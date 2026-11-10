//! Cross-row checks for an authenticated RFC-0051 snapshot. This checks
//! storage identity and accounting; the native accepting fold establishes
//! signatures, fee payment and the rendering verdict.

use crate::{
    Hash64,
    palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row},
    palw_state_v2::{PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2},
    palw_tokenizer_registration_state_v1::{TokenizerRegistrationSessionV1, TokenizerRegistrationStatusV1, packet_id},
};

/// The exact hold must come from the derived hold index for this particular
/// session key. An unrelated hold on the opener does not cover this session.
pub(crate) fn check_tokenizer_registration_snapshot_v1(
    state: &PalwChainStateV2,
    mut exact_hold: impl FnMut(PalwBondKeyV2, u64, Key) -> bool,
) -> Result<(), String> {
    let low = Hash64::default();
    let high = Hash64::from_bytes([u8::MAX; 64]);
    let sessions_from = Key::TokenizerRegistrationSession(low);
    let sessions_to = Key::TokenizerRegistrationSession(high);
    let parts_from = Key::TokenizerRegistrationPart(low, 0);
    let parts_to = Key::TokenizerRegistrationPart(high, u32::MAX);
    let sessions = || state.staged_court_range_v1(&sessions_from, &sessions_to);
    let parts = || state.staged_court_range_v1(&parts_from, &parts_to);
    let book = match state.staged_court_row_v1(&Key::TokenizerRegistrationBook) {
        Some(Row::TokenizerRegistrationBook(book)) => book,
        Some(_) => return Err("tokenizer registration book key names a foreign row".into()),
        None if sessions().next().is_none() && parts().next().is_none() => return Ok(()),
        None => return Err("tokenizer registration rows have no reservation book".into()),
    };
    let now = state.last_point().ok_or("tokenizer registration book has no accepted snapshot point")?;
    book.check()?;
    let mut session_count = 0usize;
    let mut part_count = 0usize;
    for (key, value) in sessions() {
        let (Key::TokenizerRegistrationSession(id), Row::TokenizerRegistrationSession(session)) = (key, value) else {
            return Err("tokenizer registration session range has a foreign row".into());
        };
        session_count = session_count.checked_add(1).ok_or("tokenizer snapshot session count overflows")?;
        session.check(*id)?;
        let open = &session.open.body;
        if !not_after(session.accepted, *now)
            || now.daa_score >= open.material_until_daa
            || !exact_hold(open.opener, open.material_until_daa, *key)
        {
            return Err("tokenizer session has a future point, expired storage or no exact opener hold".into());
        }
        check_pins(state, session)?;
        let reservation = book.sessions.get(id).ok_or("tokenizer session has no reservation")?;
        if book.policy != session.policy
            || (
                reservation.opener,
                reservation.accepted_daa,
                reservation.finish_until_daa,
                reservation.material_until_daa,
                reservation.quote,
            ) != (open.opener, session.accepted.daa_score, open.finish_until_daa, open.material_until_daa, session.quote)
        {
            return Err("tokenizer reservation does not match its immutable session quote".into());
        }

        let terminal = match session.status {
            TokenizerRegistrationStatusV1::Pending => None,
            TokenizerRegistrationStatusV1::Complete { point, .. }
            | TokenizerRegistrationStatusV1::Superseded { point }
            | TokenizerRegistrationStatusV1::Expired { point } => Some(point),
        };
        if terminal.is_some_and(|point| !not_after(point, *now)) || reservation.released_at != terminal {
            return Err("tokenizer terminal point differs from the reservation release or lies after the snapshot".into());
        }
        if !session.finish_attempts.is_empty() && session.received.len() != session.quote.geometry.part_count as usize {
            return Err("tokenizer Finish slot was consumed without all accepted Parts".into());
        }
        for &attempt in &session.finish_attempts {
            if book
                .assigned_slot(*id, 1, u32::from(attempt))
                .is_none_or(|daa| daa > now.daa_score || terminal.is_some_and(|point| daa > point.daa_score))
            {
                return Err("tokenizer snapshot consumes an unassigned or future Finish slot".into());
            }
        }
        match session.status {
            TokenizerRegistrationStatusV1::Complete { point, record } => {
                let registration = registration_for_session(state, session)?;
                if registration.record_id_v1() != record
                    || registration.registration_point_v1() != point
                    || !session
                        .finish_attempts
                        .iter()
                        .any(|&attempt| book.assigned_slot(*id, 1, u32::from(attempt)) == Some(point.daa_score))
                {
                    return Err("tokenizer Complete differs from its rooted registration or assigned Finish".into());
                }
            }
            TokenizerRegistrationStatusV1::Superseded { point } => {
                let registration = registration_for_session(state, session)?;
                if !not_after(registration.registration_point_v1(), point) {
                    return Err("tokenizer Superseded precedes the rooted registration".into());
                }
            }
            TokenizerRegistrationStatusV1::Pending if now.daa_score >= open.finish_until_daa => {
                return Err("tokenizer pending session passed its immutable Finish deadline".into());
            }
            TokenizerRegistrationStatusV1::Pending | TokenizerRegistrationStatusV1::Expired { .. } => {}
        }
        let mut received = session.received.iter().copied();
        for (key, value) in
            state.staged_court_range_v1(&Key::TokenizerRegistrationPart(*id, 0), &Key::TokenizerRegistrationPart(*id, u32::MAX))
        {
            let (Key::TokenizerRegistrationPart(open_id, index), Row::TokenizerRegistrationPart(part)) = (key, value) else {
                return Err("tokenizer registration Part range has a foreign row".into());
            };
            if open_id != id
                || received.next() != Some(*index)
                || !not_after(part.accepted, *now)
                || terminal.is_some_and(|point| !not_after(part.accepted, point))
                || matches!(session.status, TokenizerRegistrationStatusV1::Complete { point, .. } if part.accepted.blue_score >= point.blue_score)
                || book.assigned_slot(*id, 0, *index) != Some(part.accepted.daa_score)
            {
                return Err("tokenizer Part substitutes its received set, acceptance point or paid slot".into());
            }
            part.check(session, *index)?;
            part_count = part_count.checked_add(1).ok_or("tokenizer snapshot Part count overflows")?;
        }
        if received.next().is_some() {
            return Err("tokenizer session names missing accepted Parts".into());
        }
    }
    if session_count != book.sessions.len() || part_count != parts().count() {
        return Err("tokenizer snapshot has an orphan reservation or Part".into());
    }
    Ok(())
}

fn not_after(point: PalwBlockContextV2, snapshot: PalwBlockContextV2) -> bool {
    point.block != Hash64::default()
        && point.daa_score <= snapshot.daa_score
        && (point.blue_score < snapshot.blue_score || point == snapshot)
}

fn check_pins(state: &PalwChainStateV2, session: &TokenizerRegistrationSessionV1) -> Result<(), String> {
    let open = &session.open.body;
    let class = open.pins.class_id;
    let sem = state.class_semantics_v1(&class).ok_or("tokenizer session lost its accepted class semantics")?;
    let record = state.tir_class_v1(&class).ok_or("tokenizer session lost its registered TIR class")?;
    let container = crate::palw_tir_container_v1::palw_tir_bound_container_v1(state, class)
        .ok_or("tokenizer session lost its registered PALWART3 binding")?;
    let eof = state.staged_tokenizer_inventory_v1(open.statement.tokenizer_id).ok_or("tokenizer session lost its accepted EOF")?;
    eof.check_v1()?;
    // An old Open cannot continue under an updated prefix relation, but may
    // still be retained until its original expiry and GC. The existing class
    // semantics writer allows only a fill from None for this field.
    let mut earlier_semantics = sem.clone();
    earlier_semantics.prefix_state_relation = None;
    if (open.pins.semantics_id != sem.commitment() && open.pins.semantics_id != earlier_semantics.commitment())
        || !sem.well_formed_for(&class, &open.artifact_root)
        || sem.bound_from_daa > session.accepted.daa_score
        || sem.tokenizer_id != open.statement.tokenizer_id
        || sem.token_table_root != Some(open.statement.table_root)
        || sem.rendering_rule != Some(open.statement.rule)
        || sem.raw_tokenizer_bytes != Some(open.statement.exact_raw_bytes)
        || sem.tokenizer_key_events != Some(session.declared_key_events)
        || record.tokenizer_id != open.statement.tokenizer_id
        || record.logits_vocab != open.statement.vocab
        || (container.container_root, eof.inventory_id, eof.artifact_hash, eof.actual_bytes)
            != (open.pins.container_root, open.pins.inventory_id, open.statement.tokenizer_id, open.statement.exact_raw_bytes)
        || container.bound_daa > session.accepted.daa_score
        || eof.completed_daa > session.accepted.daa_score
    {
        return Err("tokenizer session changes its accepted class, container or EOF pins".into());
    }
    Ok(())
}

fn registration_for_session<'a>(
    state: &'a PalwChainStateV2,
    session: &TokenizerRegistrationSessionV1,
) -> Result<&'a crate::palw_tokenizer_rendering_v2::PalwRootedTokenizerRegistrationRowV1, String> {
    let pins = session.open.body.pins;
    let key = Key::VerifiedTokenizerRegistration(pins.class_id, pins.semantics_id);
    let Some(Row::VerifiedTokenizerRegistration(registration)) = state.staged_court_row_v1(&key) else {
        return Err("terminal tokenizer session has no rooted registration".into());
    };
    registration.check(&key)?;
    registration.check_context(state)?;
    if registration.facts_v1().statement != session.open.body.statement {
        return Err("terminal tokenizer session changes its rooted rendering statement".into());
    }
    Ok(registration)
}

/// Validate the forward form of one trusted native journal entry, even when
/// its application is a rollback. The post-state snapshot check separately
/// ties session updates to actual Part rows and successful registrations.
pub(crate) fn check_tokenizer_registration_delta_entry_v1(
    key: &Key,
    old: &Option<Row>,
    new: &Option<Row>,
    point: &PalwBlockContextV2,
) -> Result<bool, String> {
    let is_registration_row = |row: &Option<Row>| {
        matches!(
            row,
            Some(Row::TokenizerRegistrationSession(_) | Row::TokenizerRegistrationPart(_) | Row::TokenizerRegistrationBook(_))
        )
    };
    let foreign = || "tokenizer journal key names a foreign row or an empty transition".to_string();
    match key {
        Key::TokenizerRegistrationSession(id) => match (old, new) {
            (None, Some(Row::TokenizerRegistrationSession(row))) => {
                row.check(*id)?;
                if row.accepted != *point
                    || row.status != TokenizerRegistrationStatusV1::Pending
                    || !row.received.is_empty()
                    || !row.finish_attempts.is_empty()
                {
                    return Err("tokenizer journal Open must be empty and begin at its accepting point".into());
                }
            }
            (Some(Row::TokenizerRegistrationSession(before)), Some(Row::TokenizerRegistrationSession(after))) => {
                before.check(*id)?;
                after.check(*id)?;
                check_session_delta(before, after, *point)?;
            }
            (Some(Row::TokenizerRegistrationSession(row)), None) => {
                row.check(*id)?;
                if point.daa_score < row.open.body.material_until_daa || !not_after(row.accepted, *point) {
                    return Err("tokenizer journal deletes a session before its immutable retention deadline".into());
                }
            }
            _ => return Err(foreign()),
        },
        Key::TokenizerRegistrationPart(id, index) => match (old, new) {
            (None, Some(Row::TokenizerRegistrationPart(part))) => {
                part.check_stored(*id, *index)?;
                if part.accepted != *point {
                    return Err("tokenizer journal Part changes its accepting point".into());
                }
            }
            (Some(Row::TokenizerRegistrationPart(part)), None) => {
                part.check_stored(*id, *index)?;
                if point.daa_score < part.material_until_daa || !not_after(part.accepted, *point) {
                    return Err("tokenizer journal deletes a Part before its immutable retention deadline".into());
                }
            }
            _ => return Err(foreign()),
        },
        Key::TokenizerRegistrationBook => match (old, new) {
            (None, Some(Row::TokenizerRegistrationBook(book))) => check_book_delta(None, Some(book), *point)?,
            (Some(Row::TokenizerRegistrationBook(before)), Some(Row::TokenizerRegistrationBook(after))) => {
                check_book_delta(Some(before), Some(after), *point)?;
            }
            (Some(Row::TokenizerRegistrationBook(book)), None) => check_book_delta(Some(book), None, *point)?,
            _ => return Err(foreign()),
        },
        _ if is_registration_row(old) || is_registration_row(new) => return Err(foreign()),
        _ => return Ok(false),
    }
    Ok(true)
}

fn check_session_delta(
    before: &TokenizerRegistrationSessionV1,
    after: &TokenizerRegistrationSessionV1,
    point: PalwBlockContextV2,
) -> Result<(), String> {
    use TokenizerRegistrationStatusV1::{Complete, Expired, Pending, Superseded};
    if before.status != Pending
        || !not_after(before.accepted, point)
        || point.daa_score >= before.open.body.material_until_daa
        || before.open != after.open
        || before.policy != after.policy
        || before.quote != after.quote
        || before.accepted != after.accepted
        || before.key_hash != after.key_hash
        || before.declared_key_events != after.declared_key_events
        || !before.received.is_subset(&after.received)
        || !before.finish_attempts.is_subset(&after.finish_attempts)
    {
        return Err("tokenizer journal mutates an immutable session or loses accepted work".into());
    }
    let parts_changed = before.received != after.received;
    let attempts_changed = before.finish_attempts != after.finish_attempts;
    if (parts_changed && attempts_changed)
        || after.received.len() - before.received.len() > 1
        || after.finish_attempts.len() - before.finish_attempts.len() > 1
    {
        return Err("tokenizer journal combines more than one paid session action".into());
    }
    if (parts_changed || attempts_changed) && (point == before.accepted || point.daa_score >= before.open.body.finish_until_daa) {
        return Err("tokenizer journal advances work outside the immutable session clock".into());
    }
    if !after.finish_attempts.is_empty() && after.received.len() != after.quote.geometry.part_count as usize {
        return Err("tokenizer journal consumes a Finish slot without all Parts".into());
    }
    match after.status {
        Pending if parts_changed || attempts_changed => Ok(()),
        Complete { point: finish, .. } if finish == point && !parts_changed && attempts_changed => Ok(()),
        Superseded { point: finish } | Expired { point: finish } if finish == point && !parts_changed && !attempts_changed => Ok(()),
        _ => Err("tokenizer journal changes its terminal point or lacks a native state transition".into()),
    }
}

fn check_book_delta(
    before: Option<&crate::palw_tokenizer_registration_state_v1::TokenizerRegistrationBookV1>,
    after: Option<&crate::palw_tokenizer_registration_state_v1::TokenizerRegistrationBookV1>,
    point: PalwBlockContextV2,
) -> Result<(), String> {
    if let Some(before) = before {
        before.check()?;
        for (id, reservation) in &before.sessions {
            let Some(after) = after.filter(|book| book.sessions.contains_key(id)) else {
                if point.daa_score < reservation.material_until_daa {
                    return Err("tokenizer journal removes a reservation before retention expires".into());
                }
                continue;
            };
            let next = &after.sessions[id];
            if (next.opener, next.accepted_daa, next.finish_until_daa, next.material_until_daa, next.quote, &next.assigned_slots)
                != (
                    reservation.opener,
                    reservation.accepted_daa,
                    reservation.finish_until_daa,
                    reservation.material_until_daa,
                    reservation.quote,
                    &reservation.assigned_slots,
                )
            {
                return Err("tokenizer journal changes an existing reservation or its immutable slot history".into());
            }
            match (reservation.released_at, next.released_at) {
                (None, Some(released)) if released == point => {}
                (before, after) if before == after => {}
                _ => return Err("tokenizer journal substitutes or reopens a reservation release".into()),
            }
            if next.released_at.is_none() {
                let parts = (0..reservation.quote.geometry.part_count).map(|index| (0, index));
                for (kind, index) in parts.chain([(1, 0), (1, 1)]) {
                    let packet = packet_id(*id, kind, index)?;
                    if before.book.packets.get(&packet) != after.book.packets.get(&packet)
                        || before.book.assigned_daa(&packet) != after.book.assigned_daa(&packet)
                    {
                        return Err("tokenizer journal reschedules an existing service packet".into());
                    }
                }
            }
        }
    }
    if let Some(after) = after {
        after.check()?;
        if after.sessions.is_empty() {
            return Err("tokenizer journal retains an empty reservation book".into());
        }
        if before.is_some_and(|book| book.policy != after.policy) {
            return Err("tokenizer journal changes the reservation book policy".into());
        }
        for (id, reservation) in &after.sessions {
            if before.is_none_or(|book| !book.sessions.contains_key(id))
                && (reservation.accepted_daa != point.daa_score || reservation.released_at.is_some())
            {
                return Err("tokenizer journal inserts a reservation at a foreign accepting DAA".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn historical_snapshot_needs_no_tokenizer_book() {
        check_tokenizer_registration_snapshot_v1(&PalwChainStateV2::genesis(), |_, _, _| {
            panic!("a historical snapshot has no tokenizer hold")
        })
        .unwrap();
    }

    #[test]
    fn equal_blue_score_requires_the_exact_accepted_point() {
        let at = PalwBlockContextV2 { block: Hash64::from_u64_word(1), daa_score: 20, blue_score: 31, subsidy: 0 };
        assert!(not_after(at, at));
        assert!(!not_after(PalwBlockContextV2 { block: Hash64::from_u64_word(2), ..at }, at));
        assert!(!not_after(PalwBlockContextV2 { daa_score: 21, blue_score: 30, ..at }, at));
        assert!(not_after(PalwBlockContextV2 { daa_score: 20, blue_score: 30, ..at }, at));
    }
    fn fixture_session(nonce: u64) -> TokenizerRegistrationSessionV1 {
        use crate::palw_tokenizer_registration_v1::{TokenizerRegistrationOpenV1, tests::fixture};
        let (mut policy, mut body) = fixture();
        policy.service_budget.controls = 1;
        body.policy = policy.id_v1().unwrap();
        body.nonce = Hash64::from_u64_word(nonce);
        let accepted = PalwBlockContextV2 { block: Hash64::from_u64_word(71), daa_score: 20, blue_score: 21, subsidy: 0 };
        let declared_key_events = 10_000;
        TokenizerRegistrationSessionV1 {
            open: TokenizerRegistrationOpenV1 { body, signature: [0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN] },
            policy,
            quote: policy.quote_v1(&body, accepted.daa_score, declared_key_events).unwrap(),
            accepted,
            key_hash: Hash64::from_u64_word(72),
            declared_key_events,
            received: Default::default(),
            finish_attempts: Default::default(),
            status: TokenizerRegistrationStatusV1::Pending,
        }
    }

    #[test]
    fn tokenizer_journal_binds_session_progress_and_terminal_point() {
        use TokenizerRegistrationStatusV1::Superseded;
        let h = Hash64::from_u64_word;
        let initial = fixture_session(73);
        let key = Key::TokenizerRegistrationSession(initial.id().unwrap());
        let row = |value| Some(Row::TokenizerRegistrationSession(Box::new(value)));
        let empty = row(initial.clone());
        assert_eq!(check_tokenizer_registration_delta_entry_v1(&key, &None, &empty, &initial.accepted), Ok(true));
        assert!(check_tokenizer_registration_delta_entry_v1(&Key::TirClaim(h(74)), &None, &empty, &initial.accepted).is_err());
        let mut progressed = initial.clone();
        progressed.received.insert(0);
        let part_at = PalwBlockContextV2 { block: h(75), daa_score: 21, blue_score: 22, subsidy: 0 };
        let progress = row(progressed);
        assert_eq!(check_tokenizer_registration_delta_entry_v1(&key, &empty, &progress, &part_at), Ok(true));
        assert!(check_tokenizer_registration_delta_entry_v1(&key, &progress, &empty, &part_at).is_err());
        assert!(check_tokenizer_registration_delta_entry_v1(&key, &empty, &None, &part_at).is_err());

        let mut same_block = initial.clone();
        same_block.status = Superseded { point: initial.accepted };
        let superseded = row(same_block);
        assert_eq!(check_tokenizer_registration_delta_entry_v1(&key, &empty, &superseded, &initial.accepted), Ok(true));
        assert!(check_tokenizer_registration_delta_entry_v1(&key, &empty, &superseded, &part_at).is_err());
        assert!(check_tokenizer_registration_delta_entry_v1(&key, &superseded, &empty, &part_at).is_err());
    }

    #[test]
    fn tokenizer_journal_preserves_surviving_assignments() {
        use crate::palw_tokenizer_registration_state_v1::TokenizerRegistrationBookV1;
        let first = fixture_session(81);
        let other = fixture_session(82);
        let mut before = TokenizerRegistrationBookV1::new(first.policy).unwrap();
        before.reserve(&first).unwrap();
        let mut valid = before.clone();
        valid.reserve(&other).unwrap();
        let mut reordered = TokenizerRegistrationBookV1::new(first.policy).unwrap();
        reordered.reserve(&other).unwrap();
        reordered.reserve(&first).unwrap();
        let packet = packet_id(first.id().unwrap(), 1, 0).unwrap();
        assert_ne!(valid.book.assigned_daa(&packet), reordered.book.assigned_daa(&packet));
        let mut released = valid.clone();
        let release_at = PalwBlockContextV2 {
            block: Hash64::from_u64_word(83),
            daa_score: valid.assigned_slot(first.id().unwrap(), 1, 0).unwrap(),
            blue_score: first.accepted.blue_score + 1,
            subsidy: 0,
        };
        released.release(first.id().unwrap(), release_at).unwrap();
        assert_eq!(released.assigned_slot(first.id().unwrap(), 1, 0), Some(release_at.daa_score));
        let row = |value: crate::palw_tokenizer_registration_state_v1::TokenizerRegistrationBookV1| {
            Some(Row::TokenizerRegistrationBook(Box::new(value).into()))
        };
        let key = Key::TokenizerRegistrationBook;
        let old = row(before);
        assert_eq!(check_tokenizer_registration_delta_entry_v1(&key, &old, &row(valid.clone()), &first.accepted), Ok(true));
        assert!(check_tokenizer_registration_delta_entry_v1(&key, &old, &row(reordered), &first.accepted).is_err());
        assert!(check_tokenizer_registration_delta_entry_v1(&key, &old, &None, &first.accepted).is_err());
        let released = row(released);
        assert_eq!(check_tokenizer_registration_delta_entry_v1(&key, &row(valid.clone()), &released, &release_at), Ok(true));
        assert!(check_tokenizer_registration_delta_entry_v1(&key, &row(valid.clone()), &released, &first.accepted).is_err());
        assert!(check_tokenizer_registration_delta_entry_v1(&key, &released, &row(valid), &release_at).is_err());
    }
}
