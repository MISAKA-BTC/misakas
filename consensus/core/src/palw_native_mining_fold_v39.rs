//! **Owning39 ordinary mining in the fold** (RFC-0002 §II.14, rows 9–13): stage actions 73–79 and the native receipt-block
//! spend. A child module of `palw_state_v2`; dormant behind `Params::palw_native_mining_v39` (absent on every preset, refused when
//! armed), with native39, Final27, the ADR-0176 budget and RFC-0018 Gate C all required in force.
//!
//! ```text
//! 73 register   an immutable ordinary model over a registered complete Master template (source, classes, pipeline, task);
//!               the economic class is DERIVED; component classes are retained (HOLD_MODEL) until retirement; free (ADR-0175)
//! 74 seat       a producer's own possession seat: its own stake reserved on the shared budget (liability), never an approval
//! 75 commit     one genuine complete-Master escrow Final -> canonical work of the stages the signer produced (actual positions,
//!               every component's own source), price in the network quantum, (master, stage) used once, budget reserved
//!               (B/R/F + bond formula v2 liability, model allocation clipped), RFC-0018 'V' certification committed
//! 76 retire     kind 1 seat exit after its hold; kind 2 model retirement (registrant; no seats, claims or receipts)
//! 77 issue      decided certification: SecurityCertified -> receipt (quanta, lifetime); anything else -> closed, budget closed
//! 78            refused by name: a quantum is spent only as a receipt block's own work (`PNR9`)
//! 79 collect    kind 0 a closed claim without receipt, kind 1 an expired or exhausted receipt (and its claim)
//! PNR9 block    one receipt quantum: Gate C, strict B/R draw + 0.51 export, F clipped -> safe_weight
//! ```
//!
//! **User decisions 2026-10-15 (rounds 4–5).** There is no model-level review, exam, probation, renewal or promotion; a
//! registered model is permanently tier 0.3 from registration and genesis/annual-fork models are 1.0 — decided ONLY by
//! `palw_model_tier_v1` (ADR-0179) and applied ONCE in the allocation weight `A_m = c_m·S̄_m^1.5`
//! by the budget engine, never here and never per block. Drawn-verifier approval exists only per claim (RFC-0018 Gate C).
//! Nothing in this module grants a model genesis-equivalent standing.
use super::*;
use crate::{
    palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1},
    palw_native_execution_v39::NativeExecutionPoliciesV39,
    palw_native_master_v39::NativeMasterFinalRecordV39,
    palw_native_mining_v39::*,
    palw_native_receipt_v39::{native_mining_budget_id_v39, native_mining_receipt_view_v39, native_mining_row_key_v39},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_quota11::{DerivedOrdinaryMetadataV11, TrustedEpochV11},
    flat_resources10::{ResourceAxisV10 as A, ResourceQuantitiesV10},
    hash::{Digest, object_id},
};

const TABLE: u8 = MINING_TABLE_V39;
fn bad(s: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(s.into())
}
fn b<T: BorshSerialize>(v: &T) -> Vec<u8> {
    borsh::to_vec(v).expect("bounded mining39 row")
}
fn k<T: BorshSerialize>(kind: u8, id: &T) -> Vec<u8> {
    native_mining_row_key_v39(kind, id)
}
fn read<T: BorshDeserialize>(r: &PalwKernelRouteStateV1, kind: u8, id: &impl BorshSerialize, cap: usize) -> Option<T> {
    let raw = r.aux.get(&(TABLE, k(kind, id))).filter(|raw| raw.len() <= cap)?;
    borsh::from_slice(raw).ok()
}
fn counter(r: &PalwKernelRouteStateV1, kind: u8, id: &impl BorshSerialize) -> u64 {
    read::<u64>(r, kind, id, 8).unwrap_or(0)
}
fn route<'b>(builder: &'b TransitionBuilder<'_>) -> Result<&'b PalwKernelRouteStateV1, PalwStateV2Error> {
    builder.state.kernel_route.as_ref().ok_or_else(|| bad("mining39 kernel route absent"))
}

/// The ONE call site of a native model's tier (user decisions 2026-10-15, rounds 4–5): ADR-0179's `palw_model_tier_v1` and its
/// coefficient, consumed for reporting only — the budget engine applies it once in `A_m`, never here and never per block. A v39
/// mining model id is neither a registered class nor a model line, so the tier function reads it as `Registered` (0.3 past the
/// tier fence, 1.0 below it as for every model, A-2); only a genesis or annual-fork listing is ever the approved tier.
pub(super) trait NativeModelTierSourceV39 {
    fn tier_permille(&self, state: &PalwChainStateV2, params: &PalwStateParamsV2, model: &Hash64) -> u16;
}
pub(super) struct ModelTierV1SourceV39;
impl NativeModelTierSourceV39 for ModelTierV1SourceV39 {
    fn tier_permille(&self, state: &PalwChainStateV2, params: &PalwStateParamsV2, model: &Hash64) -> u16 {
        let daa = state.last_point().map_or(0, |p| p.daa_score);
        let tier = crate::palw_model_tier_v1::palw_model_tier_v1(state, params, model);
        crate::palw_model_tier_v1::palw_model_tier_coefficient_permille_v1(params.model_tier_v1(), tier, daa).min(1000) as u16
    }
}

pub(super) fn model(r: &PalwKernelRouteStateV1, id: &Digest) -> Result<NativeMiningModelRecordV39, PalwStateV2Error> {
    let (root, m): (Digest, NativeMiningModelRecordV39) =
        read(r, MINING_MODEL_KIND_V39, id, 16384).ok_or_else(|| bad("mining39 original model absent"))?;
    if root != m.root() || m.version != 39 || m.model != *id {
        return Err(bad("mining39 original model record differs"));
    }
    Ok(m)
}
pub(super) fn write_model(builder: &mut TransitionBuilder<'_>, m: &NativeMiningModelRecordV39) -> Result<(), PalwStateV2Error> {
    builder.write_kernel_row(TABLE, k(MINING_MODEL_KIND_V39, &m.model), Some(b(&(m.root(), m))))?;
    Ok(())
}
pub(super) fn seat(r: &PalwKernelRouteStateV1, model: &Digest, bond: &PalwBondKeyV2) -> Option<NativeMiningSeatRecordV39> {
    let (root, s): (Digest, NativeMiningSeatRecordV39) = read(r, MINING_SEAT_KIND_V39, &(model, bond), 2048)?;
    (root == s.root() && s.version == 39 && s.model == *model && s.bond == *bond).then_some(s)
}
pub(super) fn write_seat(builder: &mut TransitionBuilder<'_>, s: &NativeMiningSeatRecordV39) -> Result<(), PalwStateV2Error> {
    builder.write_kernel_row(TABLE, k(MINING_SEAT_KIND_V39, &(s.model, s.bond)), Some(b(&(s.root(), s))))?;
    Ok(())
}
pub(super) fn claim(r: &PalwKernelRouteStateV1, id: &Digest) -> Result<NativeMiningClaimRecordV39, PalwStateV2Error> {
    let (root, c): (Digest, NativeMiningClaimRecordV39) =
        read(r, MINING_CLAIM_KIND_V39, id, 16384).ok_or_else(|| bad("mining39 original claim absent"))?;
    if root != c.root() || c.version != 39 || c.id != *id {
        return Err(bad("mining39 original claim record differs"));
    }
    Ok(c)
}
pub(super) fn write_claim(builder: &mut TransitionBuilder<'_>, c: &NativeMiningClaimRecordV39) -> Result<(), PalwStateV2Error> {
    builder.write_kernel_row(TABLE, k(MINING_CLAIM_KIND_V39, &c.id), Some(b(&(c.root(), c))))?;
    Ok(())
}
fn write_receipt(builder: &mut TransitionBuilder<'_>, r: &NativeMiningReceiptRecordV39) -> Result<(), PalwStateV2Error> {
    builder.write_kernel_row(TABLE, k(MINING_RECEIPT_KIND_V39, &r.id), Some(b(&(r.root(), r))))?;
    Ok(())
}
pub(super) fn master_final(r: &PalwKernelRouteStateV1, master: &Digest) -> Option<NativeMasterFinalRecordV39> {
    let raw = r.aux.get(&(TABLE, borsh::to_vec(&(39u8, 83u8, master)).ok()?)).filter(|raw| raw.len() <= 64 << 10)?;
    let (root, f): (Digest, NativeMasterFinalRecordV39) = borsh::from_slice(raw).ok()?;
    (root == f.root() && f.version == 39 && f.master == *master).then_some(f)
}

/// The mining policy in force at `ctx`, with every prerequisite lane: native39 (the caller's `p`), Final27, the ADR-0176 budget,
/// and RFC-0018 Gate C under the certification threshold the policy states its premise against.
pub(super) fn policy(builder: &TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<NativeMiningPolicyV39, PalwStateV2Error> {
    let e = builder.extras.kernel_route.as_ref().ok_or_else(|| bad("mining39 route extras absent"))?;
    let f = e
        .native_mining39
        .as_ref()
        .filter(|f| f.activation.is_active(ctx.daa_score))
        .ok_or_else(|| bad("mining39 owning mining fence inactive"))?;
    let domain = route(builder)?.header.policy.network_domain;
    f.policy.check(domain).map_err(bad)?;
    let gate = builder.params.rfc0018_v1().ok_or_else(|| bad("mining39 requires RFC-0018 Gate C"))?;
    if !gate.gate_c_at(ctx.daa_score) {
        return Err(bad("mining39 requires RFC-0018 Gate C in force"));
    }
    f.policy.check_certification(&gate.certification).map_err(bad)?;
    if builder.bond_budget_mirror_at(ctx.daa_score).is_none() {
        return Err(bad("mining39 requires the ADR-0176 bond budget in force"));
    }
    Ok(f.policy.clone())
}
/// Bounded ordinary metadata prepayment (the same tariff shape as every owning39 action).
fn prepay(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    restore: u64,
    body: usize,
) -> Result<bool, PalwStateV2Error> {
    let bytes = restore.checked_add(body as u64).ok_or_else(|| bad("mining39 restore extent overflow"))?;
    if bytes > 128 << 20 {
        return Err(bad("mining39 metadata envelope exceeded"));
    }
    let work = bytes.checked_mul(8192).and_then(|n| n.checked_add(65536)).ok_or_else(|| bad("mining39 restore work overflow"))?;
    let Some(fee) = super::palw_flat_model_vector_fold_v3::charge(builder, ctx, signer, work, false)? else { return Ok(false) };
    builder.slash_bond(*signer, fee)?;
    let mut q = ResourceQuantitiesV10::default();
    for (axis, v) in [
        (A::MetadataWork, work as u128),
        (A::ScalarSteps, work as u128),
        (A::IndexSteps, bytes as u128 * 64),
        (A::ValidatedElements, bytes as u128),
        (A::CopiedReferenceBytes, bytes as u128 * 8),
        (A::OwnedPeakBytes, bytes as u128 * 12 + 65536),
        (A::CourtScratchBytes, bytes as u128 * 12 + 65536),
        (A::ContainerAllocatorAllowance, bytes as u128 * 4 + 65536),
        (A::Attempts, 1),
    ] {
        q.set(axis, v);
    }
    let e = TrustedEpochV11::from_chain(&p.execution.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(bad)?;
    let mut s =
        super::palw_flat_execution_fold_v11::quota_shard(builder, &p.execution, &e, palw_kernel_bond_id_v1(signer), [0; 64])?;
    s.debit_ordinary(&p.execution.quota, &e, &DerivedOrdinaryMetadataV11::from_native_envelope((&q).into())).map_err(bad)?;
    super::palw_flat_execution_fold_v11::write_shard(builder, &s, [0; 64])?;
    Ok(true)
}
fn matured(builder: &TransitionBuilder<'_>, signer: &PalwBondKeyV2, p: &NativeExecutionPoliciesV39, now: u64) -> Result<Digest, PalwStateV2Error> {
    let bond = builder.state.bonds.get(signer).ok_or_else(|| bad("mining39 signer bond absent"))?;
    if !matches!(bond.status, PalwBondStatusV2::Active)
        || now.saturating_sub(bond.registered_daa) < p.execution.execution.horizons.seal_maturity
    {
        return Err(bad("mining39 signer is not an Active matured bond"));
    }
    Ok(bond.operator_id.as_bytes())
}

pub(super) fn apply(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    action: u8,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if action == SPEND_RECEIPT_ACTION_V39 {
        return Err(bad("mining39 receipt quanta are spent only as a receipt block's own work (PNR9 carriage)"));
    }
    let policy = policy(builder, ctx)?;
    match action {
        REGISTER_MODEL_ACTION_V39 => register(builder, ctx, signer, p, &policy, body),
        MODEL_SEAT_ACTION_V39 | MODEL_RETIRE_ACTION_V39 => seat_or_retire(builder, ctx, signer, p, &policy, action, body),
        MINING_COMMIT_ACTION_V39 => commit(builder, ctx, signer, p, &policy, body),
        ISSUE_RECEIPT_ACTION_V39 => issue(builder, ctx, signer, p, &policy, body),
        COLLECT_MINING_ACTION_V39 => collect(builder, ctx, signer, p, body),
        _ => Err(bad("unknown owning39 mining action")),
    }
}

fn register(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    policy: &NativeMiningPolicyV39,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let w = NativeMiningModelWireV39::from_bytes(body).map_err(bad)?;
    let operator = matured(builder, signer, p, ctx.daa_score)?;
    let r = route(builder)?;
    let m = super::palw_native_master_fold_v39::record(r, w.template_master)?;
    let next = counter(r, MINING_MODEL_SEQUENCE_KIND_V39, signer);
    let id = native_mining_model_id_v39(m.complete_source, m.class_set, m.pipeline, m.task);
    if w.sequence != next
        || w.share_permille > policy.maximum_registrant_share_permille
        || m.policy != p.root()
        || r.aux.contains_key(&(TABLE, k(MINING_MODEL_KIND_V39, &id)))
        || r.aux.contains_key(&(TABLE, k(MINING_MODEL_SOURCE_KIND_V39, &m.complete_source)))
        || counter(r, MINING_MODEL_COUNT_KIND_V39, &()) >= policy.maximum_models as u64
    {
        return Err(bad("mining39 registration sequence/share/policy/duplicate line/model capacity differs"));
    }
    if !prepay(builder, ctx, signer, p, 65536, body.len())? {
        return Ok(());
    }
    let record = NativeMiningModelRecordV39 {
        version: 39,
        model: id,
        economic_class: native_mining_economic_class_v39(id),
        template_master: w.template_master,
        complete_source: m.complete_source,
        manifest: m.manifest,
        pipeline: m.pipeline,
        class_set: m.class_set,
        source_classes: m.components.iter().map(|c| c.class).collect(),
        original_task: m.task,
        original_contexts: m.components.iter().map(|c| c.original_context).collect(),
        registrant: *signer,
        registrant_operator: operator,
        share_permille: w.share_permille,
        admitted_daa: ctx.daa_score,
        policy: policy.root(),
        canonical_cost_table: inference_cost_table_v39(),
        seats: 0,
        live_claims: 0,
        live_receipts: 0,
        retired: None,
    };
    // Keep every component class while the model is registered; release only at retirement.
    for (stage, class) in record.source_classes.iter().enumerate() {
        super::palw_native_source_fold_v39::retain_class(
            builder,
            *class,
            super::palw_native_source_fold_v39::HOLD_MODEL,
            object_id(b"misaka-palw/native/mining-model-class-hold/v39", &(id, stage as u16)),
            u64::MAX,
        )?;
    }
    let count = counter(route(builder)?, MINING_MODEL_COUNT_KIND_V39, &()) + 1;
    write_model(builder, &record)?;
    builder.write_kernel_row(TABLE, k(MINING_MODEL_SOURCE_KIND_V39, &record.complete_source), Some(b(&id)))?;
    builder.write_kernel_row(TABLE, k(MINING_MODEL_SEQUENCE_KIND_V39, signer), Some(b(&(next + 1))))?;
    builder.write_kernel_row(TABLE, k(MINING_MODEL_COUNT_KIND_V39, &()), Some(b(&count)))?;
    Ok(())
}

fn seat_or_retire(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    policy: &NativeMiningPolicyV39,
    action: u8,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let w = NativeMiningSeatWireV39::from_bytes(body).map_err(bad)?;
    let r = route(builder)?;
    let mut m = model(r, &w.model)?;
    let seats: Vec<PalwBondKeyV2> = read(r, MINING_SEAT_LIST_KIND_V39, &w.model, 4 + 64 * 68).unwrap_or_default();
    match (action, w.kind) {
        (MODEL_SEAT_ACTION_V39, 0) => {
            let operator = matured(builder, signer, p, ctx.daa_score)?;
            let r = route(builder)?;
            if m.retired.is_some() || seat(r, &w.model, signer).is_some() || seats.len() >= 64 || seats.contains(signer) {
                return Err(bad("mining39 seat on a retired model, a repeated seat or a full seat list"));
            }
            let s = NativeMiningSeatRecordV39 {
                version: 39,
                model: w.model,
                bond: *signer,
                operator,
                stake: policy.seat_stake_sompi,
                since: ctx.daa_score,
                last_use: ctx.daa_score,
                live_claims: 0,
            };
            let free =
                (builder.state.bonds.get(signer).map(|b| b.collateral).unwrap_or(0) as u128).saturating_sub(builder.committed_at(signer, ctx.daa_score));
            if free < s.stake as u128 {
                return Err(bad("mining39 seat stake exceeds the bond's free collateral"));
            }
            if !prepay(builder, ctx, signer, p, 0, body.len())? {
                return Ok(());
            }
            // The producer's OWN stake, reserved as liability on the shared window (user-escrow origin: no model clipping).
            let plan = builder
                .plan_bond_budget_v1(
                    ctx,
                    *signer,
                    None,
                    crate::palw_bond_budget_v1::PalwBudgetAskV1 {
                        block_units: 0,
                        reward_sompi: 0,
                        final_weight: 0,
                        liability_sompi: s.stake as u128,
                    },
                    crate::palw_bond_budget_v1::PalwBudgetOriginV1::KernelRoute,
                    0,
                )?
                .ok_or_else(|| bad("mining39 seat stake reservation requires the bond budget"))?;
            builder.commit_bond_budget_v1(ctx, Hash64::from_bytes(s.budget_id()), plan)?;
            let mut seats = seats;
            seats.push(*signer);
            m.seats = m.seats.saturating_add(1);
            write_seat(builder, &s)?;
            builder.write_kernel_row(TABLE, k(MINING_SEAT_LIST_KIND_V39, &w.model), Some(b(&seats)))?;
            write_model(builder, &m)?;
            builder.note_kernel_bond(signer)?;
            Ok(())
        }
        (MODEL_RETIRE_ACTION_V39, 1) => {
            let s = seat(r, &w.model, signer).ok_or_else(|| bad("mining39 seat absent"))?;
            if s.live_claims != 0 || ctx.daa_score < s.last_use.saturating_add(policy.seat_hold_daa) {
                return Err(bad("mining39 seat still backs live claims or its hold has not elapsed"));
            }
            if !prepay(builder, ctx, signer, p, 0, body.len())? {
                return Ok(());
            }
            builder.bond_budget_close_v1(&Hash64::from_bytes(s.budget_id()), ctx.daa_score)?;
            let mut seats = seats;
            seats.retain(|b| b != signer);
            m.seats = m.seats.saturating_sub(1);
            builder.write_kernel_row(TABLE, k(MINING_SEAT_KIND_V39, &(w.model, *signer)), None)?;
            builder.write_kernel_row(TABLE, k(MINING_SEAT_LIST_KIND_V39, &w.model), Some(b(&seats)))?;
            write_model(builder, &m)?;
            Ok(())
        }
        (MODEL_RETIRE_ACTION_V39, 2) => {
            if *signer != m.registrant || m.retired.is_some() || m.seats != 0 || m.live_claims != 0 || m.live_receipts != 0 {
                return Err(bad("mining39 retirement needs the registrant and no seats, claims or receipts"));
            }
            if !prepay(builder, ctx, signer, p, 0, body.len())? {
                return Ok(());
            }
            for (stage, class) in m.source_classes.clone().iter().enumerate() {
                super::palw_native_source_fold_v39::release_class(
                    builder,
                    *class,
                    super::palw_native_source_fold_v39::HOLD_MODEL,
                    object_id(b"misaka-palw/native/mining-model-class-hold/v39", &(w.model, stage as u16)),
                )?;
            }
            m.retired = Some(ctx.daa_score);
            write_model(builder, &m)?;
            Ok(())
        }
        _ => Err(bad("mining39 seat/retire action and kind differ")),
    }
}

fn commit(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    policy: &NativeMiningPolicyV39,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let w = NativeMiningCommitWireV39::from_bytes(body).map_err(bad)?;
    let operator = matured(builder, signer, p, ctx.daa_score)?;
    let r = route(builder)?;
    let mut m = model(r, &w.model)?;
    let mut s = seat(r, &w.model, signer).ok_or_else(|| bad("mining39 producer holds no possession seat for the model"))?;
    let next = counter(r, MINING_PRODUCER_SEQUENCE_KIND_V39, signer);
    let fin = master_final(r, &w.master).ok_or_else(|| bad("mining39 complete Master escrow Final absent"))?;
    let master = super::palw_native_master_fold_v39::record(r, w.master)?;
    let epoch = ctx.daa_score / policy.epoch_daa;
    let epoch_count = counter(r, MINING_EPOCH_KIND_V39, &(w.model, epoch));
    if !m.admits_claims(ctx.daa_score, policy.model_maturity_daa)
        || w.sequence != next
        || master.complete_source != m.complete_source
        || master.class_set != m.class_set
        || master.pipeline != m.pipeline
        || fin.original_record != master.root()
        || fin.established > ctx.daa_score
        || ctx.daa_score > fin.established.saturating_add(policy.claim_window_daa)
        || epoch_count >= policy.maximum_claims_per_model_epoch as u64
        || (m.live_claims as u64 + m.live_receipts as u64) >= policy.maximum_live_receipts_per_model as u64
    {
        return Err(bad("mining39 commit model/sequence/Master line/Final window/epoch or live capacity differs"));
    }
    // The stages this producer actually produced, each credited at most once ever.
    let mut stages = Vec::new();
    let mut restore = 65536u64;
    for c in &fin.components {
        let h = super::palw_native_job_fold_v39::header(r, c.original.job)?;
        if h.original.producer != *signer {
            continue;
        }
        if r.aux.contains_key(&(TABLE, k(MINING_MASTER_USED_KIND_V39, &(w.master, c.original.stage)))) {
            return Err(bad("mining39 a Master stage is credited at most once"));
        }
        // G-N2: the component's material must still be retained (its collection 53 not yet run) for a post-Final accusation.
        if super::palw_native_job_fold_v39::native_job_collected_v39(r, c.original.job)? {
            return Err(bad("mining39 a credited component's material is already collected"));
        }
        restore = restore.saturating_add(h.source_bytes as u64);
        stages.push((c.original.stage, h.original.class, c.original.job));
    }
    if stages.is_empty() {
        return Err(bad("mining39 the signer produced no component of this Master"));
    }
    if !prepay(builder, ctx, signer, p, restore, body.len())? {
        return Ok(());
    }
    // Canonical work from each component's ACTUAL accepted positions under its own source.
    let r = route(builder)?;
    let mut checked = Vec::with_capacity(stages.len());
    for (_, class, job) in &stages {
        let h = super::palw_native_job_fold_v39::header(r, *job)?;
        let book = super::palw_native_job_fold_v39::book(r, &h)?;
        let positions = book.claim().ok_or_else(|| bad("mining39 Final component without its accepted claim"))?.positions;
        checked.push((super::palw_native_source_fold_v39::restore_class(r, p, *class)?, positions));
    }
    let pairs = checked.iter().map(|(c, positions)| (c.source(), *positions)).collect::<Vec<_>>();
    let work = derive_component_work_v39(&pairs).map_err(bad)?;
    let mut price = NativeMiningPriceV39::derive(
        w.model,
        w.master,
        work.root(),
        work.mac_equivalents,
        policy.network_quantum_mac,
        policy.network_slash_per_mac,
    )
    .map_err(bad)?;
    // Allocation clipping of the quantum count: never more than the policy ceiling per receipt.
    price.quanta = price.quanta.min(policy.maximum_quanta_per_receipt);
    let id = native_mining_claim_id_v39(w.model, w.master, signer, w.sequence);
    let model_hash = Hash64::from_bytes(w.model);
    // ADR-0176 shared budget (B/R/F, bond formula v2 liability, ADR-0177 allocation for this model line) — refused whole, never
    // silently reduced; planned BEFORE any write.
    let plan = builder.plan_native_mining_budget_v39(ctx, *signer, model_hash, price.quanta, policy.weight_per_quantum, price.reserved_exposure)?;
    let record = NativeMiningClaimRecordV39 {
        version: 39,
        id,
        model: w.model,
        master: w.master,
        master_final: fin.root(),
        master_established: fin.established,
        complete_source: m.complete_source,
        stages: stages.iter().map(|(stage, _, _)| *stage).collect(),
        producer: *signer,
        operator,
        admitted_daa: ctx.daa_score,
        epoch,
        policy: policy.root(),
        work,
        price,
        closed: None,
    };
    // ---- writes ----
    builder.commit_bond_budget_v1(ctx, native_mining_budget_id_v39(&id), plan)?;
    super::palw_security_certification_fold_v1::commit_native_mining_v39(
        builder,
        ctx,
        &Hash64::from_bytes(id),
        &model_hash,
        signer,
        fin.established,
    )?;
    for (stage, _, _) in &stages {
        builder.write_kernel_row(TABLE, k(MINING_MASTER_USED_KIND_V39, &(w.master, *stage)), Some(b(&id)))?;
    }
    // G-N2: hold every credited component's material through the claim's liability (collection 53 refused meanwhile).
    let certification = builder.params.rfc0018_v1().map(|m| m.certification).ok_or_else(|| bad("mining39 requires RFC-0018"))?;
    let until = palw_native_mining_liability_until_v39(ctx.daa_score, policy, &certification);
    for (_, _, job) in &stages {
        builder.write_kernel_row(TABLE, k(MINING_JOB_HOLD_KIND_V39, job), Some(b(&(id, until))))?;
    }
    m.live_claims += 1;
    s.live_claims += 1;
    s.last_use = ctx.daa_score;
    write_claim(builder, &record)?;
    write_model(builder, &m)?;
    write_seat(builder, &s)?;
    builder.write_kernel_row(TABLE, k(MINING_PRODUCER_SEQUENCE_KIND_V39, signer), Some(b(&(next + 1))))?;
    builder.write_kernel_row(TABLE, k(MINING_EPOCH_KIND_V39, &(w.model, epoch)), Some(b(&(epoch_count + 1))))?;
    builder.note_kernel_bond(signer)?;
    Ok(())
}

fn issue(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    policy: &NativeMiningPolicyV39,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let w = NativeMiningSubjectWireV39::from_bytes(body).map_err(bad)?;
    if w.kind != 0 {
        return Err(bad("mining39 issue kind"));
    }
    let r = route(builder)?;
    let mut c = claim(r, &w.id)?;
    if c.closed.is_some() {
        return Err(bad("mining39 claim already decided"));
    }
    let hash = c.claim_hash();
    let row = crate::palw_security_certification_v1::palw_native_mining_certification_row_v39(&builder.state, &hash)
        .ok_or_else(|| bad("mining39 certification row absent"))?;
    let Some((outcome, decided)) = row.outcome else { return Err(bad("mining39 certification not decided yet")) };
    if !prepay(builder, ctx, signer, p, 0, body.len())? {
        return Ok(());
    }
    let r = route(builder)?;
    let mut m = model(r, &c.model)?;
    let mut s = seat(r, &c.model, &c.producer);
    m.live_claims = m.live_claims.saturating_sub(1);
    if let Some(s) = s.as_mut() {
        s.live_claims = s.live_claims.saturating_sub(1);
    }
    let certified = outcome.certifies()
        && crate::palw_rfc0018_v1::palw_native_independently_work_certified_v39(builder.params, &builder.state, &hash, c.admitted_daa);
    if certified {
        let quanta = c.price.quanta;
        let receipt = NativeMiningReceiptRecordV39 {
            version: 39,
            id: native_mining_receipt_id_v39(c.id),
            claim: c.id,
            model: c.model,
            master: c.master,
            producer: c.producer,
            operator: c.operator,
            approval: object_id(b"misaka-palw/native/mining-certification-approval/v39", &(row.claim, decided, row.draws.len() as u32, row.positive_positions())),
            price: c.price.clone(),
            established_daa: ctx.daa_score,
            expires: ctx.daa_score.saturating_add(policy.receipt_lifetime_daa),
            spent_quanta: 0,
            spent: vec![0; (quanta as usize).div_ceil(8)],
        };
        m.live_receipts += 1;
        c.closed = Some((ctx.daa_score, true));
        write_receipt(builder, &receipt)?;
    } else {
        // Not certified (or not certifiable): no receipt, no weight, no coinbase; the reservation closes (nothing returns).
        c.closed = Some((ctx.daa_score, false));
        builder.bond_budget_close_v1(&native_mining_budget_id_v39(&c.id), ctx.daa_score)?;
    }
    write_claim(builder, &c)?;
    write_model(builder, &m)?;
    if let Some(s) = s {
        write_seat(builder, &s)?;
    }
    Ok(())
}

fn collect(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    p: &NativeExecutionPoliciesV39,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    let w = NativeMiningSubjectWireV39::from_bytes(body).map_err(bad)?;
    let r = route(builder)?;
    let (claim_id, receipt) = match w.kind {
        0 => {
            let c = claim(r, &w.id)?;
            if c.closed.map(|(_, issued)| issued) != Some(false) {
                return Err(bad("mining39 only a decided claim without receipt is collected directly"));
            }
            (c.id, None)
        }
        _ => {
            let (rc, c) =
                native_mining_receipt_view_v39(&builder.state, &Hash64::from_bytes(w.id)).ok_or_else(|| bad("mining39 receipt absent"))?;
            if ctx.daa_score <= rc.expires && rc.spent_quanta < rc.price.quanta {
                return Err(bad("mining39 receipt still live"));
            }
            // G-N2: a rewarded claim stays accusable (and its material held) through its liability horizon.
            let policy = policy(builder, ctx)?;
            let certification = builder.params.rfc0018_v1().map(|m| m.certification).ok_or_else(|| bad("mining39 requires RFC-0018"))?;
            if ctx.daa_score <= palw_native_mining_liability_until_v39(c.admitted_daa, &policy, &certification) {
                return Err(bad("mining39 receipt's claim is still within its liability horizon"));
            }
            (c.id, Some(rc))
        }
    };
    if !prepay(builder, ctx, signer, p, 0, body.len())? {
        return Ok(());
    }
    let r = route(builder)?;
    let c = claim(r, &claim_id)?;
    let mut m = model(r, &c.model)?;
    if let Some(rc) = &receipt {
        m.live_receipts = m.live_receipts.saturating_sub(1);
        builder.bond_budget_close_v1(&native_mining_budget_id_v39(&c.id), ctx.daa_score)?;
        builder.write_kernel_row(TABLE, k(MINING_RECEIPT_KIND_V39, &rc.id), None)?;
    }
    // G-N2: the claim leaves its liability: its material holds and its attestors' locks are released uncharged.
    super::palw_native_job_fold_v39::release_mining_holds_v39(builder, &c)?;
    super::palw_security_certification_fold_v1::release_native_attestor_locks_v39(builder, &c.claim_hash())?;
    if let Some(rc) = &receipt {
        builder.write_kernel_row(TABLE, k(MINING_RECEIPT_GRANTED_KIND_V39, &rc.id), None)?;
    }
    // A permanent tombstone keeps the claim id (and its used Master stages) from ever being re-credited.
    builder.write_kernel_row(TABLE, k(MINING_COLLECTED_KIND_V39, &c.id), Some(b(&(c.root(), c.closed, ctx.daa_score))))?;
    builder.write_kernel_row(TABLE, k(MINING_CLAIM_KIND_V39, &c.id), None)?;
    super::palw_security_certification_fold_v1::retire_native_mining_v39(builder, &c.claim_hash())?;
    write_model(builder, &m)?;
    Ok(())
}

/// **The receipt block's own work** (`PNR9`, routed from the shared receipt-spend site): one quantum of a live, Gate-C-certified
/// receipt, drawn strictly from its budget (one block unit and this block's carve, within the 0.51 export) with its weight
/// clipped, credited to `safe_weight`. The coinbase pays this block's worker share as for every entitled receipt block.
pub(super) fn apply_native_receipt_block_v39(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    spend: &crate::palw_freeprompt_v3::PalwReceiptSpendUnsignedV3,
) -> Result<(), PalwStateV2Error> {
    let policy = policy(builder, ctx)?;
    let receipt_id = spend.claim_id;
    let (mut rc, c) = native_mining_receipt_view_v39(&builder.state, &receipt_id).ok_or_else(|| bad("mining39 receipt absent"))?;
    if rc.producer.0 != spend.producer_bond
        || ctx.daa_score > rc.expires
        || spend.quantum_index as u64 >= rc.price.quanta
        || rc.is_spent(spend.quantum_index)
    {
        return Err(bad("mining39 receipt block producer/expiry/quantum or single use differs"));
    }
    if !crate::palw_rfc0018_v1::palw_native_independently_work_certified_v39(builder.params, &builder.state, &c.claim_hash(), c.admitted_daa) {
        return Err(bad("mining39 receipt block's mining claim is not SecurityCertified (Gate C)"));
    }
    if crate::palw_native_receipt_v39::native_mining_accusation_pending_v39(&builder.state, &c.claim_hash()) {
        return Err(bad("mining39 receipt block's mining claim has an open post-Final accusation (G-N2)"));
    }
    // RFC-0008 once-only settlement (RFC08-1; dormant `palw_settlement_once_v1`): the quantum's worker share settles once across
    // every consumer, whatever the receipt's own bitmap says — checked and written before the budget draw; a repeat is refused.
    {
        use crate::palw_settlement_once_v1::{
            PalwSettlementConsumerV1, PalwSettlementDebitV1, PalwSettlementLegV1, PalwSettlementObligationV1,
        };
        let carve = builder.params.worker_carve_at(ctx.subsidy, builder.extras.escrow_carve);
        let obligation = PalwSettlementObligationV1::with_party(
            PalwSettlementLegV1::NativeReceiptSpend,
            receipt_id.as_bytes(),
            [0; 64],
            u64::from(spend.quantum_index),
        );
        // RFC08-2: a native mining claim is not a V2 claim — X3 defines no hard deadline for it and it carries no signed lifetime
        // — so its receipt quantum has no liability terms to source: refused past the fence, never defaulted.
        let once = builder.settle_once_liable_v1(
            ctx.daa_score,
            obligation,
            PalwSettlementConsumerV1::ReceiptBlock,
            carve,
            PalwSettlementDebitV1::Issuance { carve },
            super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::Unsourced(
                "a native mining receipt: its mining claim carries no signed hard deadline (X3 governs V2 claims only)",
            ),
        )?;
        match once {
            super::palw_settlement_once_fold_v1::PalwSettleOnceV1::AlreadySettled(_) => {
                return Err(bad("mining39 receipt quantum already settled by another consumer"));
            }
            super::palw_settlement_once_fold_v1::PalwSettleOnceV1::Refused(why) => {
                return Err(super::palw_settlement_liability_fold_v1::liability_refused_v1(why));
            }
            _ => {}
        }
    }
    let last = rc.spent_quanta + 1 >= rc.price.quanta;
    let granted =
        builder.bond_budget_native_receipt_spend_v39(ctx, &native_mining_budget_id_v39(&c.id), rc.producer, policy.weight_per_quantum, last)?;
    builder.state.safe_weight = builder.state.safe_weight.checked_add(granted).ok_or(PalwStateV2Error::Overflow("safe_weight"))?;
    // G-N2: what this receipt granted, so a conviction takes all of it back.
    let total: u128 = read::<u128>(route(builder)?, MINING_RECEIPT_GRANTED_KIND_V39, &rc.id, 32).unwrap_or(0);
    builder.write_kernel_row(TABLE, k(MINING_RECEIPT_GRANTED_KIND_V39, &rc.id), Some(b(&total.saturating_add(granted))))?;
    rc.spent[spend.quantum_index as usize / 8] |= 1 << (spend.quantum_index % 8);
    rc.spent_quanta += 1;
    write_receipt(builder, &rc)?;
    Ok(())
}

/// The certification sweep's standing fact for a native mining claim: its claim row is undecided and its complete Master escrow
/// Final still stands.
pub(super) fn native_mining_claim_live_v39(state: &PalwChainStateV2, mining_claim: &Hash64) -> bool {
    let Some(r) = state.kernel_route.as_ref() else { return false };
    claim(r, &mining_claim.as_bytes()).is_ok_and(|c| c.closed.is_none() && master_final(r, &c.master).is_some_and(|f| f.root() == c.master_final))
}
/// Readonly view for RPC/SDK: a model and its tier coefficient (permille) as `palw_model_tier_v1` reports it.
pub fn palw_native_mining_model_view_v39(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    model_id: &Digest,
) -> Option<(NativeMiningModelRecordV39, u16)> {
    let r = state.kernel_route.as_ref()?;
    let m = model(r, model_id).ok()?;
    let tier = ModelTierV1SourceV39.tier_permille(state, params, &Hash64::from_bytes(*model_id));
    Some((m, tier))
}
