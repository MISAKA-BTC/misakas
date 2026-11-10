//! Bounded read models for an authenticated TKR1 parent.
use super::*;
use crate::{
    palw_native_resource_v1::{NativeCoverageV1, NativeRetainedV1, reserve_native_pending_requests_v1},
    palw_tokenizer_registration_v1::TokenizerRegistrationQuoteV1,
};
use borsh::BorshSerialize;

/// A price and capacity check for an unsigned Open at the prospective point.
/// It does not return service slots, reserve capacity, or verify UTXO funding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
pub struct TokenizerRegistrationOpenQuoteReadV1 {
    pub open_id: Hash64,
    pub policy: TokenizerRegistrationPolicyV1,
    pub declared_key_events: u64,
    pub quote: TokenizerRegistrationQuoteV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
pub struct TokenizerRegistrationPartReadV1 {
    pub index: u32,
    pub offset: u64,
    pub bytes: u32,
    pub slot_daa: u64,
    pub accepted: Option<PalwBlockContextV2>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
pub struct TokenizerRegistrationFinishReadV1 {
    pub slot_daa: u64,
    pub attempted: bool,
}

/// Accepted Open and its original immutable slot assignments. Slots remain
/// visible as history after release. Quote charge is the registered Open's
/// service burn; the row does not record its carrier ID or gross wallet fee.
#[derive(Debug, PartialEq, Eq, BorshSerialize)]
pub struct TokenizerRegistrationStatusReadV1 {
    pub open: TokenizerRegistrationOpenBodyV1,
    pub open_id: Hash64,
    pub accepted: PalwBlockContextV2,
    pub quote: TokenizerRegistrationQuoteV1,
    pub charge_id: Hash64,
    pub status: Status,
    pub released_at: Option<PalwBlockContextV2>,
    pub parts: Vec<TokenizerRegistrationPartReadV1>,
    /// Normal, then recovery.
    pub finishes: [TokenizerRegistrationFinishReadV1; 2],
}

/// Use only a native authenticated selected parent and its prospective point.
/// The accepting fold will recheck the actual parent, prefix, fees and signature.
pub fn palw_tokenizer_registration_open_quote_v1(
    parent: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    at: &PalwBlockContextV2,
    extras: &PalwTransitionExtrasV1,
    open: &TokenizerRegistrationOpenBodyV1,
) -> Result<Option<TokenizerRegistrationOpenQuoteReadV1>, PalwStateV2Error> {
    let Some(policy) = extras.tokenizer_registration_policy_v1 else { return Ok(None) };
    let network = extras
        .tokenizer_registration_network_domain_v1
        .ok_or_else(|| refusal("tokenizer quote lacks its native network domain".into()))?;
    let open_id = open.id_v1().map_err(refusal)?;
    if at.block == Hash64::default() {
        return Err(refusal("tokenizer quote requires a prospective point".into()));
    }
    match parent.staged_court_row_v1(&Key::TokenizerRegistrationSession(open_id)) {
        Some(Row::TokenizerRegistrationSession(session)) if at.daa_score < session.open.body.material_until_daa => {
            return Err(refusal("tokenizer Open is already retained; read its accepted status".into()));
        }
        Some(Row::TokenizerRegistrationSession(_)) | None => {}
        Some(_) => return Err(refusal("tokenizer Open names a foreign row".into())),
    }
    check_new_tokenizer_open_v1(parent, extras, at, open)?;
    let (_, declared_key_events, quote) =
        TokenizerRegistrationSessionV1::prepare_unsigned_v1(parent, params, extras, *at, network, policy, open).map_err(refusal)?;

    let existing = match parent.staged_court_row_v1(&Key::TokenizerRegistrationBook) {
        Some(Row::TokenizerRegistrationBook(book)) => Some(book.as_ref()),
        None => None,
        Some(_) => return Err(refusal("tokenizer service book has a foreign type".into())),
    };
    let _scratch = tokenizer_construction_quote_v1(parent, policy, None)?.reserve_scratch_v1()?;
    let _previous_scratch = existing
        .filter(|book| book.policy != policy)
        .map(|book| tokenizer_construction_quote_v1(parent, book.policy, None)?.reserve_scratch_v1())
        .transpose()?;
    let mut book = if let Some(existing) = existing {
        let mut book = existing.clone();
        for (key, row) in parent.staged_court_range_v1(
            &Key::TokenizerRegistrationSession(Hash64::default()),
            &Key::TokenizerRegistrationSession(Hash64::from_bytes([u8::MAX; 64])),
        ) {
            let (Key::TokenizerRegistrationSession(id), Row::TokenizerRegistrationSession(session)) = (key, row) else {
                return Err(refusal("tokenizer quote found a foreign session".into()));
            };
            advance_tokenizer_book_session_v1(&mut book, *id, session, at)?;
        }
        if book.sessions.is_empty() && book.policy != policy {
            TokenizerRegistrationBookV1::new(policy).map_err(refusal)?
        } else {
            book
        }
    } else {
        TokenizerRegistrationBookV1::new(policy).map_err(refusal)?
    };
    book.reserve_open_v1(open_id, open, at.daa_score, policy, quote).map_err(refusal)?;
    Ok(Some(TokenizerRegistrationOpenQuoteReadV1 { open_id, policy, declared_key_events, quote }))
}

/// Read only the requested Open in the authenticated native snapshot. Absence
/// can also mean GC or reorg; there is no historical rejection tombstone.
pub fn palw_tokenizer_registration_status_v1(
    parent: &PalwChainStateV2,
    open_id: Hash64,
) -> Result<Option<NativeRetainedV1<TokenizerRegistrationStatusReadV1>>, PalwStateV2Error> {
    let session = match parent.staged_court_row_v1(&Key::TokenizerRegistrationSession(open_id)) {
        Some(Row::TokenizerRegistrationSession(session)) => session,
        None => return Ok(None),
        Some(_) => return Err(refusal("tokenizer status found a foreign session".into())),
    };
    let Some(Row::TokenizerRegistrationBook(book)) = parent.staged_court_row_v1(&Key::TokenizerRegistrationBook) else {
        return Err(refusal("tokenizer status lacks its reservation book".into()));
    };
    let reservation = book.session_reservation_v1(open_id, session).map_err(refusal)?;
    let open = session.open.body;
    let terminal_point = match session.status {
        Status::Pending => None,
        Status::Complete { point, record } => {
            let pins = open.pins;
            let key = Key::VerifiedTokenizerRegistration(pins.class_id, pins.semantics_id);
            if !matches!(parent.staged_court_row_v1(&key), Some(Row::VerifiedTokenizerRegistration(row))
                if row.record_id_v1() == record && row.registration_point_v1() == point)
            {
                return Err(refusal("tokenizer completion lacks its rooted registration".into()));
            }
            Some(point)
        }
        Status::Superseded { point } | Status::Expired { point } => Some(point),
    };
    if terminal_point != reservation.released_at {
        return Err(refusal("tokenizer status changes the original reservation release".into()));
    }
    let count = session.quote.geometry.part_count;
    let bytes = u64::from(count).checked_mul(std::mem::size_of::<TokenizerRegistrationPartReadV1>() as u64).ok_or_else(|| {
        PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<TokenizerRegistrationPartReadV1>("tokenizer status Parts"))
    })?;
    let lease = reserve_native_pending_requests_v1(bytes).map_err(PalwStateV2Error::NativeResource)?;
    let mut parts = Vec::new();
    parts.try_reserve_exact(count as usize).map_err(|why| {
        PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<TokenizerRegistrationPartReadV1>(
            "tokenizer status Parts",
            &why.to_string(),
        ))
    })?;
    if parts.capacity() != count as usize {
        return Err(PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<TokenizerRegistrationPartReadV1>(
            "tokenizer status Parts",
            "allocation exceeds the exact reservation",
        )));
    }
    if let Some(lease) = &lease {
        lease.materialized_v1();
    }
    let slot = |kind, index| {
        book.assigned_slot(open_id, kind, index).ok_or_else(|| refusal("tokenizer status lacks an immutable service slot".into()))
    };
    for index in 0..count {
        let (offset, bytes) = session.quote.geometry.part_v1(index).map_err(|why| refusal(why.into()))?;
        let slot_daa = slot(0, index)?;
        let accepted = match parent.staged_court_row_v1(&Key::TokenizerRegistrationPart(open_id, index)) {
            Some(Row::TokenizerRegistrationPart(part)) if session.received.contains(&index) && part.accepted.daa_score == slot_daa => {
                Some(part.accepted)
            }
            None if !session.received.contains(&index) => None,
            _ => return Err(refusal("tokenizer status Part receipt differs from its retained row or slot".into())),
        };
        parts.push(TokenizerRegistrationPartReadV1 { index, offset, bytes: bytes as u32, slot_daa, accepted });
    }
    let finish = |index| {
        Ok::<_, PalwStateV2Error>(TokenizerRegistrationFinishReadV1 {
            slot_daa: slot(1, u32::from(index))?,
            attempted: session.finish_attempts.contains(&index),
        })
    };
    let status = TokenizerRegistrationStatusReadV1 {
        open,
        open_id,
        accepted: session.accepted,
        quote: session.quote,
        charge_id: open.charge_id_v1().map_err(refusal)?,
        status: session.status.clone(),
        released_at: reservation.released_at,
        parts,
        finishes: [finish(0)?, finish(1)?],
    };
    Ok(Some(NativeRetainedV1::from_owner_reserved_v1(status, lease)))
}
