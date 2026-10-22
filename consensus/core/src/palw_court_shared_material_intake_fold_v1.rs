//! RFC-0032 A / ADR-0199 material intake fold (lane material-intake). Rooted
//! rows written through the ordinary staged writer, so block deltas, reorg undo,
//! carriage restart and the material-horizon sweep apply unchanged.
//!
//! Dormant: refused unless `palw_staged_tir_route_v1` is in force at the block
//! (`extras.staged_tir_route_active`, the dormant G-10 fence the shared TIR
//! court lives under), the original has an installed shared context, and the
//! storage fee schedule resolves — it is typed-missing in every non-test build.
//! No lifecycle dispatcher/wire tag calls this yet (see lane notes).
use super::*;
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
use crate::palw_court_shared_tir_state_v1 as shared;
use crate::palw_court_shared_material_intake_v1 as m;
use crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as PART;

fn refused(s: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.into())
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum MaterialIntakeEffectV1 {
    /// A new draft for this literal submitter.
    Begun,
    /// Exact replay of an accepted Begin/Part: no state change.
    Retried,
    /// One more part appended to this submitter's own draft.
    Part,
    /// This body was the first complete authenticated one: it is canonical now.
    Completed,
    /// Another body already completed this target; this draft was released.
    AlreadyComplete,
}

pub(super) fn material_intake(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2, s: &m::SignedMaterialIntakeV1)
    -> Result<MaterialIntakeEffectV1, PalwStateV2Error> {
    let saved = b.try_checkpoint()?;
    let result = material_intake_inner(b, ctx, s);
    if result.is_err() {
        b.restore(saved);
    }
    result
}

fn material_intake_inner(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2, s: &m::SignedMaterialIntakeV1)
    -> Result<MaterialIntakeEffectV1, PalwStateV2Error> {
    if !b.extras.staged_tir_route_active {
        return Err(refused("material intake is dormant below palw_staged_tir_route_v1"));
    }
    let schedule = m::material_intake_schedule_v1().map_err(|_| refused("material intake storage fee schedule is not adopted"))?;
    let network = b.extras.tir_public_network_domain_v1.ok_or_else(|| refused("material intake native network domain missing"))?;
    let c = shared::context(&b.state, s.claim).map_err(refused)?.clone();
    if ctx.daa_score < c.original_accepted_daa || ctx.daa_score >= c.hard_daa {
        return Err(refused("material intake outside original hard interval"));
    }
    if s.scope_id != c.scope_id {
        return Err(refused("material intake native scope mismatch"));
    }
    // Any eligible party: an Active bond at/above the existing floor. No Ready,
    // owner slot, panel seat or first-filer status is required or granted.
    let bond = b.state.bond(&s.submitter).ok_or(PalwStateV2Error::MissingBond(s.submitter))?;
    if !matches!(bond.status, PalwBondStatusV2::Active) || bond.collateral < b.params.min_collateral_sompi() {
        return Err(refused("material submitter is not active/above existing floor"));
    }
    s.verify_v1(network, &bond.pubkey).map_err(refused)?;
    let source = c.source(&b.state).map_err(refused)?;
    let complete = m::target_complete_v1(&b.state, s.claim, s.target).map_err(refused)?;
    match &s.action {
        m::MaterialActionV1::Begin { total_bytes, body_hash } => {
            let draft_id = m::material_draft_id_v1(s.claim, s.target, s.submitter, *total_bytes, *body_hash).map_err(refused)?;
            let key = Key::CourtMaterialDraft(s.claim, draft_id);
            match b.state.staged_court_row_v1(&key) {
                Some(Row::CourtMaterialDraft(old)) => {
                    if ctx.daa_score < old.begun_daa {
                        return Err(refused("material Begin replay predates accepted draft"));
                    }
                    return Ok(MaterialIntakeEffectV1::Retried);
                }
                Some(_) => return Err(refused("material draft key holds a foreign row")),
                None => {}
            }
            if complete {
                return Ok(MaterialIntakeEffectV1::AlreadyComplete);
            }
            let program = m::class_program_v1(&b.state, s.claim).map_err(refused)?;
            let leaves = m::required_parameter_leaves_v1(&b.state, s.claim, schedule.cover).map_err(refused)?;
            let bound = m::target_bound_v1(&b.state, &c, &source, s.target, &program, &leaves).map_err(refused)?;
            if *total_bytes == 0 || *total_bytes > bound {
                return Err(refused("material declaration exceeds the original's native requirement"));
            }
            let lkey = Key::CourtMaterialLedger(s.claim, s.submitter);
            let mut ledger = match b.state.staged_court_row_v1(&lkey) {
                Some(Row::CourtMaterialLedger(l)) => (**l).clone(),
                None => m::MaterialLedgerV1 {
                    version: 1, claim: s.claim, submitter: s.submitter,
                    budget_bytes: m::claim_budget_v1(&b.state, &c, &source, &program, &leaves).map_err(refused)?,
                    reserved_bytes: 0, charged_sompi: 0, material_until_daa: c.material_until_daa,
                },
                Some(_) => return Err(refused("material ledger key holds a foreign row")),
            };
            ledger.reserved_bytes = ledger.reserved_bytes.checked_add(*total_bytes)
                .filter(|r| *r <= ledger.budget_bytes)
                .ok_or_else(|| refused("material declaration exceeds the submitter's per-claim native budget"))?;
            let fee = schedule.sompi_per_byte.checked_mul(*total_bytes).ok_or_else(|| refused("material fee overflow"))?;
            ledger.charged_sompi = ledger.charged_sompi.checked_add(fee).ok_or_else(|| refused("material fee overflow"))?;
            let expires_daa = ctx.daa_score.checked_add(schedule.draft_ttl_daa).ok_or_else(|| refused("material TTL overflow"))?
                .min(c.hard_daa).min(c.material_until_daa);
            if expires_daa <= ctx.daa_score {
                return Err(refused("material draft would expire at once"));
            }
            let draft = m::MaterialDraftV1 {
                version: 1, claim: s.claim, scope_id: c.scope_id, target: s.target, submitter: s.submitter, draft_id,
                total_bytes: *total_bytes, body_hash: *body_hash, begun_block: ctx.block, begun_daa: ctx.daa_score,
                last_daa: ctx.daa_score, accepted_bytes: 0, head: Hash64::default(), expires_daa,
                material_until_daa: c.material_until_daa,
            };
            b.write_staged_row_v1(lkey, Some(Row::CourtMaterialLedger(Box::new(ledger))))?;
            b.write_staged_row_v1(key, Some(Row::CourtMaterialDraft(Box::new(draft))))?;
            Ok(MaterialIntakeEffectV1::Begun)
        }
        m::MaterialActionV1::Part { draft_id, offset, bytes } => {
            let mut draft = own_draft(b, s, *draft_id)?;
            if ctx.daa_score >= draft.expires_daa || ctx.daa_score < draft.last_daa {
                return Err(refused("material part outside its draft's window"));
            }
            let part_key = Key::CourtMaterialPart(*draft_id, *offset);
            if let Some(Row::CourtMaterialPart(part)) = b.state.staged_court_row_v1(&part_key) {
                if part.bytes != *bytes {
                    return Err(refused("material part retry changed bytes"));
                }
                return Ok(MaterialIntakeEffectV1::Retried);
            }
            if complete {
                return Err(refused("material target already complete; no further storage"));
            }
            let end = crate::palw_court_shared_tir_stream_v1::check_shared_stream_part_v1(draft.total_bytes, draft.accepted_bytes,
                *offset, bytes.len()).map_err(refused)?;
            let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
            q.exact_v1::<u8>(bytes.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
            let _raw = crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
            let part = m::MaterialPartV1 {
                version: 1, claim: s.claim, draft_id: *draft_id, offset: *offset, previous: draft.head,
                bytes: bytes.clone(), daa: ctx.daa_score, expires_daa: draft.expires_daa,
            };
            draft.head = m::material_part_digest_v1(&part).map_err(refused)?;
            draft.accepted_bytes = end;
            draft.last_daa = ctx.daa_score;
            b.write_staged_row_v1(part_key, Some(Row::CourtMaterialPart(Box::new(part).into())))?;
            b.write_staged_row_v1(Key::CourtMaterialDraft(s.claim, *draft_id), Some(Row::CourtMaterialDraft(Box::new(draft))))?;
            Ok(MaterialIntakeEffectV1::Part)
        }
        m::MaterialActionV1::Finish { draft_id } => {
            let draft = own_draft(b, s, *draft_id)?;
            if complete {
                // First complete authenticated body already won; this one is released.
                release_draft(b, &draft)?;
                return Ok(MaterialIntakeEffectV1::AlreadyComplete);
            }
            if ctx.daa_score >= draft.expires_daa || draft.accepted_bytes != draft.total_bytes {
                return Err(refused("material Finish of an expired or incomplete draft"));
            }
            let body = draft.body(&b.state).map_err(refused)?;
            if crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body) != draft.body_hash {
                return Err(refused("material body checksum mismatch"));
            }
            match s.target {
                m::MaterialTargetV1::FullIds => {
                    let ids = crate::palw_court_shared_tir_stream_state_v1::decode_ids_body(&source, &body).map_err(refused)?;
                    super::palw_court_shared_tir_fold_v1::share_ids(b, ctx, s.claim, ids.rows_root_v1(), ids.ids_v1())?;
                }
                target => {
                    let leaves = match target {
                        m::MaterialTargetV1::Parameter { .. } =>
                            Some(m::required_parameter_leaves_v1(&b.state, s.claim, schedule.cover).map_err(refused)?),
                        _ => None,
                    };
                    m::decode_material_v1(&b.state, &c, &source, s.claim, target, &body, leaves.as_deref()).map_err(refused)?;
                    let target_id = target.id_v1(s.claim).map_err(refused)?;
                    let row = m::MaterialRowV1 {
                        version: 1, claim: s.claim, scope_id: c.scope_id, target, target_id,
                        digest: draft.body_hash, bytes: body, block: ctx.block, accepted_daa: ctx.daa_score,
                        completed_by: s.submitter, material_until_daa: c.material_until_daa,
                    };
                    b.write_staged_row_v1(Key::CourtMaterial(s.claim, target_id), Some(Row::CourtMaterial(Box::new(row).into())))?;
                }
            }
            release_draft(b, &draft)?;
            Ok(MaterialIntakeEffectV1::Completed)
        }
    }
}

fn own_draft(b: &TransitionBuilder<'_>, s: &m::SignedMaterialIntakeV1, draft_id: Hash64) -> Result<m::MaterialDraftV1, PalwStateV2Error> {
    let Some(Row::CourtMaterialDraft(draft)) = b.state.staged_court_row_v1(&Key::CourtMaterialDraft(s.claim, draft_id)) else {
        return Err(refused("material draft missing"));
    };
    if draft.submitter != s.submitter || draft.target != s.target || draft.scope_id != s.scope_id {
        return Err(refused("material draft belongs to another submitter/target"));
    }
    Ok((**draft).clone())
}

/// Removes a draft and its parts and returns its reservation. The ledger's
/// accounting of the charged schedule price is kept (payer unadopted).
pub(super) fn release_draft(b: &mut TransitionBuilder<'_>, draft: &m::MaterialDraftV1) -> Result<(), PalwStateV2Error> {
    let mut at = 0u64;
    while at < draft.accepted_bytes {
        let key = Key::CourtMaterialPart(draft.draft_id, at);
        if b.state.staged_court_row_v1(&key).is_some() {
            b.write_staged_row_v1(key, None)?;
        }
        at = at.checked_add(PART as u64).ok_or_else(|| refused("material release overflow"))?;
    }
    let dkey = Key::CourtMaterialDraft(draft.claim, draft.draft_id);
    if b.state.staged_court_row_v1(&dkey).is_some() {
        b.write_staged_row_v1(dkey, None)?;
    }
    let lkey = Key::CourtMaterialLedger(draft.claim, draft.submitter);
    if let Some(Row::CourtMaterialLedger(l)) = b.state.staged_court_row_v1(&lkey) {
        let mut l = (**l).clone();
        l.reserved_bytes = l.reserved_bytes.saturating_sub(draft.total_bytes);
        b.write_staged_row_v1(lkey, Some(Row::CourtMaterialLedger(Box::new(l))))?;
    }
    Ok(())
}

/// Bounded explicit expiry for one claim (the sweep also releases expired drafts).
pub(super) fn expire_material_drafts(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2, claim: Hash64)
    -> Result<usize, PalwStateV2Error> {
    let (lo, hi) = (Hash64::from_bytes([0u8; 64]), Hash64::from_bytes([0xffu8; 64]));
    let expired: Vec<m::MaterialDraftV1> = b.state
        .staged_court_range_v1(&Key::CourtMaterialDraft(claim, lo), &Key::CourtMaterialDraft(claim, hi))
        .filter_map(|(_, row)| match row {
            Row::CourtMaterialDraft(d) if ctx.daa_score >= d.expires_daa => Some((**d).clone()),
            _ => None,
        })
        .take(64)
        .collect();
    for draft in &expired {
        release_draft(b, draft)?;
    }
    Ok(expired.len())
}

impl TransitionBuilder<'_> {
    /// Structural guard for every write to the intake namespace: canonical
    /// material and parts are immutable once written; a canonical row must
    /// re-authenticate; a draft can only advance its own cursor.
    pub(super) fn check_material_intake_write_v1(&mut self, key: &Key, new: Option<&Row>) -> Result<(), PalwStateV2Error> {
        let ours = matches!(key, Key::CourtMaterialDraft(..) | Key::CourtMaterialPart(..) | Key::CourtMaterial(..) | Key::CourtMaterialLedger(..));
        let ours_row = matches!(new, Some(Row::CourtMaterialDraft(_) | Row::CourtMaterialPart(_) | Row::CourtMaterial(_) | Row::CourtMaterialLedger(_)));
        if !ours && !ours_row {
            return Ok(());
        }
        let Some(row) = new else {
            // Removal: canonical material leaves only at its material horizon (sweep).
            return Ok(());
        };
        row.check(key).map_err(refused)?;
        let old = self.state.staged_court_row_v1(key);
        match (row, old) {
            (Row::CourtMaterial(_) | Row::CourtMaterialPart(_), Some(old)) if old != row => {
                Err(refused("material canonical body / part is immutable"))
            }
            (Row::CourtMaterial(m), None) => m.check_context(&self.state).map_err(refused),
            (Row::CourtMaterialDraft(d), Some(Row::CourtMaterialDraft(o))) => {
                if (d.claim, d.target, d.submitter, d.total_bytes, d.body_hash, d.begun_daa, d.expires_daa)
                    != (o.claim, o.target, o.submitter, o.total_bytes, o.body_hash, o.begun_daa, o.expires_daa)
                    || d.accepted_bytes < o.accepted_bytes || d.last_daa < o.last_daa {
                    return Err(refused("material draft cannot be replaced or rewound"));
                }
                d.check_context(&self.state).map_err(refused)
            }
            (Row::CourtMaterialDraft(d), None) => {
                if d.accepted_bytes != 0 {
                    return Err(refused("material draft must start empty"));
                }
                d.check_context(&self.state).map_err(refused)
            }
            (Row::CourtMaterialLedger(l), Some(Row::CourtMaterialLedger(o))) => {
                if (l.budget_bytes, l.material_until_daa) != (o.budget_bytes, o.material_until_daa) || l.charged_sompi < o.charged_sompi {
                    return Err(refused("material ledger budget is derived once and charges never shrink"));
                }
                Ok(())
            }
            // Byte-identical rewrite of an immutable row, or a fresh part/ledger.
            (Row::CourtMaterial(_) | Row::CourtMaterialPart(_), Some(_)) => Ok(()),
            (Row::CourtMaterialPart(_) | Row::CourtMaterialLedger(_), None) => Ok(()),
            _ => Err(refused("material key holds a foreign row")),
        }
    }
}
