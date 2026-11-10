//! A single reserved scratch state per candidate, with body journal rollback.
//! No carrier or rejected body clones the unrelated accepted chain state.
use super::*;
use crate::palw_native_resource_v1::NativeTransitionReservationV1;

struct WorkingParts {
    state: PalwChainStateV2,
    entries: Vec<PalwDeltaEntryV2>,
    reservation: NativeTransitionReservationV1,
}

pub struct TokenizerRegistrationFeeWorkingStateV1 {
    parts: Option<WorkingParts>,
    pending_body: Option<(usize, u64)>,
}

impl TokenizerRegistrationFeeWorkingStateV1 {
    pub fn new(
        parent: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        point: &PalwBlockContextV2,
        extras: &PalwTransitionExtrasV1,
    ) -> Result<Self, PalwStateV2Error> {
        let owner = palw_v2_pre_tokenizer_registration_base_retained_v1(parent, params, point, extras)?;
        let (state, reservation) = NativeTransitionReservationV1::take_state_v1(owner).map_err(PalwStateV2Error::NativeResource)?;
        Ok(Self { parts: Some(WorkingParts { state, entries: Vec::new(), reservation }), pending_body: None })
    }

    fn take_builder<'a>(
        &mut self,
        params: &'a PalwStateParamsV2,
        extras: &'a PalwTransitionExtrasV1,
    ) -> Result<TransitionBuilder<'a>, PalwStateV2Error> {
        let parts = self.parts.take().ok_or_else(|| refusal("tokenizer fee scratch is poisoned after a failed mutation".into()))?;
        let mut builder =
            TransitionBuilder::from_native_parts_v1(parts.state, parts.reservation, params, false, false, false, false, extras);
        builder.entries = parts.entries;
        Ok(builder)
    }

    fn put_builder(&mut self, builder: TransitionBuilder<'_>) {
        self.parts = Some(WorkingParts { state: builder.state, entries: builder.entries, reservation: builder.native_reservation });
    }

    /// An uncommitted previous body is undone before the next body can read
    /// its scratch. Early returns may discard this entire candidate safely.
    /// Only native TKR rows can appear in this journal; they use the same
    /// derived-index writer in both directions.
    pub fn begin_body_v1(&mut self, params: &PalwStateParamsV2, extras: &PalwTransitionExtrasV1) -> Result<(), PalwStateV2Error> {
        if let Some((checkpoint, requests)) = self.pending_body.take() {
            let mut builder = self.take_builder(params, extras)?;
            while builder.entries.len() > checkpoint {
                let Some(PalwDeltaEntryV2::StagedCourtSeed { key, old, new }) = builder.entries.pop() else {
                    return Err(refusal("foreign entry in tokenizer fee rollback".into()));
                };
                if builder.state.staged_courts.get(&key) != new.as_ref() {
                    return Err(refusal("tokenizer fee rollback disagrees with scratch".into()));
                }
                // Forward reservations are still live: the original owner
                // covers the restored rows and index nodes; the writer's
                // old/new index projection scratch is symmetric. Move the
                // old row back without cloning or allocating another journal.
                // Do not release any of those tickets until undo is complete.
                palw_staged_court_fold_v1::update_indices(&mut builder.state, key, new.as_ref(), old.as_ref());
                match old {
                    Some(row) => {
                        builder.state.staged_courts.insert(key, row);
                    }
                    None => {
                        builder.state.staged_courts.remove(&key);
                    }
                }
            }
            // No journal survives a body boundary. Dropping its storage is
            // required before returning the forward operation's scratch ticket.
            builder.entries = Vec::new();
            builder.native_reservation.restore_requests_v1(requests).map_err(PalwStateV2Error::NativeResource)?;
            self.put_builder(builder);
        }
        let parts = self.parts.as_ref().ok_or_else(|| refusal("tokenizer fee scratch is absent".into()))?;
        self.pending_body = Some((parts.entries.len(), parts.reservation.requests_v1()));
        Ok(())
    }

    pub fn commit_body_v1(&mut self) {
        self.pending_body = None;
        // The committed rows remain in scratch. Only the active body needs
        // an undo history; discard its storage after commit.
        if let Some(parts) = &mut self.parts {
            parts.entries = Vec::new();
        }
    }

    pub fn apply_v1(
        &mut self,
        parent: &PalwChainStateV2,
        parent_root: Hash64,
        params: &PalwStateParamsV2,
        point: &PalwBlockContextV2,
        carrier: &ValidatedTokenizerRegistrationCarrierV1,
        extras: &PalwTransitionExtrasV1,
    ) -> Result<(TokenizerRegistrationFeeV1, Option<String>), PalwStateV2Error> {
        if self.pending_body.is_none() {
            return Err(refusal("tokenizer fee carrier has no active body trial".into()));
        }
        let mut builder = self.take_builder(params, extras)?;
        let previous_requests = builder.native_reservation.requests_v1();
        let previous_entries = builder.entries.len();
        let previous_capacity = builder.entries.capacity();
        let result = builder.apply_tokenizer_registration_at_v1(parent, parent_root, point, carrier);
        if result.is_ok() && builder.entries.len() == previous_entries && builder.entries.capacity() == previous_capacity {
            builder.native_reservation.restore_requests_v1(previous_requests).map_err(PalwStateV2Error::NativeResource)?;
        }
        self.put_builder(builder);
        result
    }

    #[cfg(test)]
    pub(crate) fn state_v1(&self) -> &PalwChainStateV2 {
        &self.parts.as_ref().expect("test scratch owner").state
    }
}
