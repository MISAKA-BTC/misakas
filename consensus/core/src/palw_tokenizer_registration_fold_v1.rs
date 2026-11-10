//! RFC-0051 native paid registration writer. No public activation preset.
use super::*;
use crate::{
    palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row},
    palw_tokenizer_registration_carriage_v1::{
        TokenizerRegistrationCarrierContextV1, TokenizerRegistrationFeeV1, ValidatedTokenizerRegistrationCarrierV1,
    },
    palw_tokenizer_registration_state_v1::{
        TokenizerRegistrationBookV1, TokenizerRegistrationSessionV1, TokenizerRegistrationStatusV1 as Status,
    },
    palw_tokenizer_registration_v1::{TokenizerRegistrationPolicyV1, TokenizerRegistrationWireV1 as Wire},
    palw_tokenizer_rendering_v2::PalwVerifiedTokenizerRegistrationErrorV1,
};

fn refusal(why: String) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(why)
}
fn rendering_error(why: PalwVerifiedTokenizerRegistrationErrorV1) -> PalwStateV2Error {
    match why {
        PalwVerifiedTokenizerRegistrationErrorV1::Capacity(capacity) => {
            PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<
                TokenizerRegistrationSessionV1,
            >("tokenizer registration", &capacity.to_string()))
        }
        other => refusal(other.to_string()),
    }
}

impl TransitionBuilder<'_> {
    /// Context and fee authority originate in the native UTXO validator. This
    /// consumes the handoff, and produces a fee effect only after the insertion.
    /// Semantic refusal is an ordinary paid carrier with zero registration burn.
    pub(super) fn apply_tokenizer_registration_carrier_v1(
        &mut self,
        parent: &PalwChainStateV2,
        parent_root: Hash64,
        context: TokenizerRegistrationCarrierContextV1,
        policy: TokenizerRegistrationPolicyV1,
        txid: crate::tx::TransactionId,
        handoff: crate::palw_native_resource_v1::NativeRetainedV1<ValidatedTokenizerRegistrationCarrierV1>,
    ) -> Result<(TokenizerRegistrationFeeV1, Option<String>), PalwStateV2Error> {
        if self.state.last_point() != parent.last_point()
            || context.parent_root != parent_root
            || context.policy != policy.id_v1().map_err(refusal)?
        {
            return Err(refusal("tokenizer carrier changes its native accepted parent or policy".into()));
        }
        let wire = handoff.wire_at_v1(&context, txid).map_err(refusal)?;
        let result = self.fold_tokenizer_registration_v1(parent, &context, policy, txid, &handoff, wire);
        let (inserted, rejected) = match result {
            Ok(id) => (id, None),
            Err(error @ PalwStateV2Error::NativeResource(_)) => return Err(error),
            Err(PalwStateV2Error::CarriageInconsistent(reason)) => (None, Some(reason)),
            Err(error) => return Err(error),
        };
        let session = match inserted {
            Some(id) => match self.state.staged_court_row_v1(&Key::TokenizerRegistrationSession(id)) {
                Some(Row::TokenizerRegistrationSession(session)) => Some(session.as_ref()),
                _ => return Err(refusal("new tokenizer Open lost its accepted row before fee settlement".into())),
            },
            None => None,
        };
        let fee = ValidatedTokenizerRegistrationCarrierV1::settle_after_fold_v1(handoff, &context, txid, session).map_err(refusal)?;
        Ok((fee, rejected))
    }

    fn fold_tokenizer_registration_v1(
        &mut self,
        parent: &PalwChainStateV2,
        context: &TokenizerRegistrationCarrierContextV1,
        policy: TokenizerRegistrationPolicyV1,
        txid: crate::tx::TransactionId,
        handoff: &ValidatedTokenizerRegistrationCarrierV1,
        wire: &Wire,
    ) -> Result<Option<Hash64>, PalwStateV2Error> {
        let at = context.at;
        self.reserve_tokenizer_construction_v1(policy, Some(wire))?;
        match wire {
            Wire::Open(open) => {
                let id = open.body.id_v1().map_err(refusal)?;
                let key = Key::TokenizerRegistrationSession(id);
                if self.state.staged_court_row_v1(&key).is_some() {
                    return Ok(None);
                }
                let pins = open.body.pins;
                if self.state.staged_court_row_v1(&Key::VerifiedTokenizerRegistration(pins.class_id, pins.semantics_id)).is_some() {
                    return Err(refusal("tokenizer class/semantics is already registered".into()));
                }
                let session = TokenizerRegistrationSessionV1::prepare(
                    parent,
                    self.params,
                    self.extras,
                    at,
                    context.network,
                    policy,
                    open.clone(),
                )
                .map_err(refusal)?;
                handoff.check_open_fee_v1(context, txid, &session).map_err(refusal)?;
                let mut book = match self.state.staged_court_row_v1(&Key::TokenizerRegistrationBook) {
                    Some(Row::TokenizerRegistrationBook(book)) => book.as_ref().clone(),
                    None => TokenizerRegistrationBookV1::new(policy).map_err(refusal)?,
                    _ => return Err(refusal("tokenizer service book has a foreign type".into())),
                };
                // A policy switch is permitted only after every prior retained
                // session has been garbage-collected, never by repricing it.
                if book.sessions.is_empty() && book.policy != policy {
                    book = TokenizerRegistrationBookV1::new(policy).map_err(refusal)?;
                }
                book.reserve(&session).map_err(refusal)?;
                self.commit_tokenizer_rows_v1(vec![
                    (key, Some(Row::TokenizerRegistrationSession(Box::new(session)))),
                    (Key::TokenizerRegistrationBook, Some(Row::TokenizerRegistrationBook(Box::new(book).into()))),
                ])?;
                Ok(Some(id))
            }
            Wire::Part(part) => {
                let id = part.body.open_id;
                let key = Key::TokenizerRegistrationPart(id, part.body.index);
                if let Some(existing) = self.state.staged_court_row_v1(&key) {
                    let Row::TokenizerRegistrationPart(existing) = existing else {
                        return Err(refusal("tokenizer Part key has a foreign row".into()));
                    };
                    if existing.part.body != part.body || existing.part.bytes != part.bytes {
                        return Err(refusal("tokenizer duplicate Part changes its bytes or scope".into()));
                    }
                    if existing.part.signature != part.signature {
                        let Some(Row::TokenizerRegistrationSession(session)) =
                            self.state.staged_court_row_v1(&Key::TokenizerRegistrationSession(id))
                        else {
                            return Err(refusal("tokenizer duplicate Part lost its Open".into()));
                        };
                        let bond = self
                            .state
                            .bond(&part.body.opener)
                            .ok_or_else(|| refusal("tokenizer duplicate Part lost its signing key".into()))?;
                        let key_hash = crate::palw_court_shared_tir_stream_v1::stream_digest(
                            b"misaka-palw/tokenizer-registration/opener-key/v1",
                            &bond.pubkey.as_slice(),
                        )
                        .map_err(refusal)?;
                        if key_hash != session.key_hash {
                            return Err(refusal("tokenizer duplicate Part changes its opener key".into()));
                        }
                        part.verify_signature_v1(&bond.pubkey).map_err(refusal)?;
                    }
                    return Ok(None);
                }
                let session_key = Key::TokenizerRegistrationSession(id);
                let Some(Row::TokenizerRegistrationSession(session)) = self.state.staged_court_row_v1(&session_key) else {
                    return Err(refusal("tokenizer Part has no accepted Open".into()));
                };
                let Some(Row::TokenizerRegistrationBook(book)) = self.state.staged_court_row_v1(&Key::TokenizerRegistrationBook)
                else {
                    return Err(refusal("tokenizer Part has no service book".into()));
                };
                let row = session.prepare_part(&self.state, self.params, at, part.clone(), book).map_err(refusal)?;
                let mut session = session.as_ref().clone();
                session.received.insert(part.body.index);
                self.commit_tokenizer_rows_v1(vec![
                    (key, Some(Row::TokenizerRegistrationPart(Box::new(row).into()))),
                    (session_key, Some(Row::TokenizerRegistrationSession(Box::new(session)))),
                ])?;
                Ok(None)
            }
            Wire::Finish(finish) => {
                let id = finish.body.open_id;
                let key = Key::TokenizerRegistrationSession(id);
                let Some(Row::TokenizerRegistrationSession(session)) = self.state.staged_court_row_v1(&key) else {
                    return Err(refusal("tokenizer Finish has no Open".into()));
                };
                if !matches!(session.status, Status::Pending) {
                    return Ok(None);
                }
                let Some(Row::TokenizerRegistrationSession(prior_session)) = parent.staged_court_row_v1(&key) else {
                    return Err(refusal("tokenizer Finish requires an accepted parent Open".into()));
                };
                // Same-block updates cannot supply missing parts or clear an
                // already consumed Finish attempt from an earlier carrier.
                if session != prior_session {
                    return Err(refusal("tokenizer Finish cannot consume same-block changes".into()));
                }
                let Some(Row::TokenizerRegistrationBook(book)) = parent.staged_court_row_v1(&Key::TokenizerRegistrationBook) else {
                    return Err(refusal("tokenizer Finish has no accepted parent book".into()));
                };
                let pins = session.open.body.pins;
                if self.state.class_semantics_v1(&pins.class_id) != parent.class_semantics_v1(&pins.class_id)
                    || self.state.staged_court_row_v1(&Key::TirContainer(pins.class_id))
                        != parent.staged_court_row_v1(&Key::TirContainer(pins.class_id))
                    || self.state.staged_court_row_v1(&Key::Inventory(session.open.body.statement.tokenizer_id))
                        != parent.staged_court_row_v1(&Key::Inventory(session.open.body.statement.tokenizer_id))
                    || self.state.staged_court_row_v1(&Key::VerifiedTokenizerRegistration(pins.class_id, pins.semantics_id)).is_some()
                {
                    return Err(refusal("tokenizer Finish changes accepted prerequisites or repeats an installed result".into()));
                }
                let parts: Result<Vec<_>, _> = parent
                    .staged_court_range_v1(&Key::TokenizerRegistrationPart(id, 0), &Key::TokenizerRegistrationPart(id, u32::MAX))
                    .map(|(_, row)| match row {
                        Row::TokenizerRegistrationPart(part) => Ok(part.as_ref()),
                        _ => Err(refusal("tokenizer Part row has a foreign type".into())),
                    })
                    .collect();
                let (slot, derived) =
                    session.derive_finish(parent, self.params, self.extras, at, finish, book, &parts?).map_err(rendering_error)?;
                if let Err(error @ PalwVerifiedTokenizerRegistrationErrorV1::Capacity(_)) = derived {
                    return Err(rendering_error(error));
                }
                let derived = derived.and_then(|registration| {
                    registration.check_adoption(&self.state, &at).map_err(PalwVerifiedTokenizerRegistrationErrorV1::Evidence)?;
                    Ok(registration)
                });
                let mut session = session.as_ref().clone();
                session.finish_attempts.insert(slot);
                let mut changes = Vec::new();
                let mut next_book = match self.state.staged_court_row_v1(&Key::TokenizerRegistrationBook) {
                    Some(Row::TokenizerRegistrationBook(book)) => book.as_ref().clone(),
                    _ => return Err(refusal("tokenizer Finish lost its service book".into())),
                };
                let mut book_changed = false;
                let failure = match derived {
                    Ok(registration) => {
                        let pins = session.open.body.pins;
                        let reg_key = Key::VerifiedTokenizerRegistration(pins.class_id, pins.semantics_id);
                        if self.state.staged_court_row_v1(&reg_key).is_some() {
                            return Err(refusal("tokenizer registration already has a winner".into()));
                        }
                        session.status = Status::Complete { point: at, record: registration.record_id_v1() };
                        next_book
                            .release(id, at)
                            .map_err(|_| PalwStateV2Error::DeltaMismatch("native tokenizer completion release failed"))?;
                        book_changed = true;
                        changes.push((reg_key, Some(Row::VerifiedTokenizerRegistration(Box::new(registration)))));
                        for (other_key, row) in self.state.staged_court_range_v1(
                            &Key::TokenizerRegistrationSession(Hash64::default()),
                            &Key::TokenizerRegistrationSession(Hash64::from_bytes([255; 64])),
                        ) {
                            if let Row::TokenizerRegistrationSession(other) = row {
                                if *other_key != key
                                    && matches!(other.status, Status::Pending)
                                    && (other.open.body.pins.class_id, other.open.body.pins.semantics_id)
                                        == (pins.class_id, pins.semantics_id)
                                {
                                    let mut other = other.as_ref().clone();
                                    other.status = Status::Superseded { point: at };
                                    next_book
                                        .release(other.id().map_err(refusal)?, at)
                                        .map_err(|_| PalwStateV2Error::DeltaMismatch("native tokenizer superseded release failed"))?;
                                    changes.push((*other_key, Some(Row::TokenizerRegistrationSession(Box::new(other)))));
                                }
                            }
                        }
                        None
                    }
                    Err(error) => Some(error.to_string()),
                };
                if book_changed {
                    changes.push((Key::TokenizerRegistrationBook, Some(Row::TokenizerRegistrationBook(Box::new(next_book).into()))));
                }
                changes.push((key, Some(Row::TokenizerRegistrationSession(Box::new(session)))));
                self.commit_tokenizer_rows_v1(changes)?;
                if let Some(failure) = failure {
                    return Err(refusal(failure));
                }
                Ok(None)
            }
        }
    }
}

impl TransitionBuilder<'_> {
    pub(super) fn sweep_tokenizer_registrations_v1(&mut self, at: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
        let Some(Row::TokenizerRegistrationBook(existing)) = self.state.staged_court_row_v1(&Key::TokenizerRegistrationBook) else {
            return Ok(());
        };
        let policy = existing.policy;
        self.reserve_tokenizer_construction_v1(policy, None)?;
        let Some(Row::TokenizerRegistrationBook(existing)) = self.state.staged_court_row_v1(&Key::TokenizerRegistrationBook) else {
            unreachable!()
        };
        let mut book = existing.as_ref().clone();
        let mut changes = Vec::new();
        let mut book_changed = false;
        for (key, row) in self.state.staged_court_range_v1(
            &Key::TokenizerRegistrationSession(Hash64::default()),
            &Key::TokenizerRegistrationSession(Hash64::from_bytes([255; 64])),
        ) {
            let (Key::TokenizerRegistrationSession(id), Row::TokenizerRegistrationSession(session)) = (key, row) else {
                return Err(refusal("tokenizer expiry found a foreign session".into()));
            };
            if at.daa_score >= session.open.body.material_until_daa {
                book.expire(*id, at.daa_score).map_err(refusal)?;
                book_changed = true;
                for (part_key, _) in self
                    .state
                    .staged_court_range_v1(&Key::TokenizerRegistrationPart(*id, 0), &Key::TokenizerRegistrationPart(*id, u32::MAX))
                {
                    changes.push((*part_key, None));
                }
                changes.push((*key, None));
            } else if matches!(session.status, Status::Pending) && at.daa_score >= session.open.body.finish_until_daa {
                let mut session = session.as_ref().clone();
                session.status = Status::Expired { point: *at };
                book.release(*id, *at).map_err(refusal)?;
                book_changed = true;
                changes.push((*key, Some(Row::TokenizerRegistrationSession(Box::new(session)))));
            }
        }
        if book_changed {
            changes.push((
                Key::TokenizerRegistrationBook,
                if book.sessions.is_empty() { None } else { Some(Row::TokenizerRegistrationBook(Box::new(book).into())) },
            ));
        }
        self.commit_tokenizer_rows_v1(changes)
    }
}

impl TransitionBuilder<'_> {
    /// Peak construction ownership, separately from retained rows/journal and
    /// the parser lease. Counts derive from finite policy cardinalities and
    /// concrete container element layouts, before any graph or row is cloned.
    fn reserve_tokenizer_construction_v1(
        &mut self,
        policy: TokenizerRegistrationPolicyV1,
        wire: Option<&Wire>,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_court_service_v1::{ServiceCost, ServicePacket, causal::CausalServicePacketV1};
        use crate::palw_tokenizer_registration_state_v1::{TokenizerRegistrationPartRowV1, TokenizerRegistrationReservationV1};
        let overflow = || refusal("tokenizer construction geometry overflow".into());
        let n = policy
            .max_raw_bytes
            .div_ceil(crate::palw_tokenizer_registration_v1::TOKENIZER_REGISTRATION_PART_BYTES_V1 as u64)
            .checked_add(2)
            .ok_or_else(overflow)?;
        let sessions = u64::from(policy.max_pending_total).checked_add(1).ok_or_else(overflow)?;
        let packets = sessions.checked_mul(n).ok_or_else(overflow)?;
        let mut q = native_writers::NativeWriteQuoteV1::new("tokenizer construction peak ownership");
        if let Some(wire) = wire {
            q.copy(wire)?.copy(wire)?;
        }
        if let Some(Row::TokenizerRegistrationBook(book)) = self.state.staged_court_row_v1(&Key::TokenizerRegistrationBook) {
            // Outer fold, transactional Book candidate, causal candidate;
            // each can coexist with its validation/release candidate.
            for _ in 0..6 {
                q.copy(book.as_ref())?;
            }
        }
        for (_, row) in self.state.staged_court_range_v1(
            &Key::TokenizerRegistrationSession(Hash64::default()),
            &Key::TokenizerRegistrationSession(Hash64::from_bytes([255; 64])),
        ) {
            q.copy(row)?.copy(row)?;
        }
        // New packet growth in the outer/candidate/causal book, plus rebuilt
        // remaining quota during validation. Existing nodes are above.
        for _ in 0..3 {
            q.member::<[u8; 64], ServicePacket>(n)?.member::<[u8; 64], u64>(n)?.member::<u64, ServiceCost>(n)?;
            q.member::<Hash64, TokenizerRegistrationReservationV1>(1)?.grown::<u64>(n)?;
        }
        q.member::<u64, ServiceCost>(packets)?;
        // One outer graph, one Book validation graph, one causal verification
        // graph and one release/expiry graph may coexist. Each has at most two
        // Part predecessor lists and one Finish-to-recovery edge.
        for _ in 0..4 {
            q.grown::<CausalServicePacketV1>(n)?.grown::<[u8; 64]>(n)?;
            q.grown::<[u8; 64]>(n)?.grown::<[u8; 64]>(n)?;
        }
        // Causal scheduler nodes/pending/children/ready, per-node duplicate
        // predecessor set, validation graph membership, and whole-book IDs.
        q.member::<[u8; 64], &CausalServicePacketV1>(n)?.member::<[u8; 64], usize>(n)?;
        q.member::<[u8; 64], Vec<[u8; 64]>>(n)?.grown::<[u8; 64]>(n.checked_mul(4).ok_or_else(overflow)?)?;
        q.member::<(u64, [u8; 64]), ()>(n)?.member::<[u8; 64], ()>(n.checked_mul(2).ok_or_else(overflow)?)?;
        q.member::<[u8; 64], ()>(packets)?.member::<PalwBondKeyV2, u32>(sessions)?;
        // New session, received/attempt memberships, ordered Part references,
        // superseded session changes, and all deletions in a bounded GC pass.
        q.exact::<TokenizerRegistrationSessionV1>(1)?.member::<u32, ()>(n)?.member::<u8, ()>(2)?;
        q.grown::<&TokenizerRegistrationPartRowV1>(n)?;
        q.grown::<(Key, Option<Row>)>(packets.checked_add(sessions).and_then(|n| n.checked_add(2)).ok_or_else(overflow)?)?;
        q.exact::<u8>(1024)?; // fixed ID tuple's Borsh hashing scratch
        self.admit_native_write_v1(q)
    }
}
