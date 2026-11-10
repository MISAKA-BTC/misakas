//! Strict read-only registration view. Reconstruction runs outside the virtual
//! guard; the selected view and import epoch are rechecked before publishing.
use super::*;
use kaspa_consensus_core::{
    api::TokenizerRegistrationReadSnapshotV1,
    palw_native_resource_v1::NativeRetainedV1,
    palw_state_v2::{
        PalwBlockContextV2, PalwChainStateV2, PalwStateParamsV2, PalwStateV2Error, PalwTransitionExtrasV1,
        TokenizerRegistrationOpenQuoteReadV1, TokenizerRegistrationStatusReadV1,
    },
    palw_tokenizer_registration_v1::TokenizerRegistrationOpenBodyV1,
};

impl VirtualStateProcessor {
    fn tokenizer_registration_read_v1<T>(
        &self,
        read: impl FnOnce(&PalwChainStateV2, &PalwStateParamsV2, &PalwBlockContextV2, &PalwTransitionExtrasV1) -> Result<T, String>,
    ) -> Result<Option<TokenizerRegistrationReadSnapshotV1<T>>, String> {
        let Some(params) = self.palw_state_params_v2.as_ref() else { return Ok(None) };
        let captured = self.virtual_stores.read().state.get().map_err(|e| e.to_string())?;
        if self.palw_lifecycle_kind_fences.tokenizer_registration_v1_at(captured.daa_score).is_none() {
            return Ok(None);
        }
        let prepared = self.prepare_template_parent_v1(&captured).map_err(|e| format!("tokenizer parent unavailable: {e:?}"))?;
        prepared.checked.map_err(|e| format!("tokenizer parent is not authenticated: {e:?}"))?;
        let (target, parent) = prepared.state.as_ref().ok_or("tokenizer parent is unavailable")?;
        let parent_point = parent.last_point().copied().ok_or("tokenizer parent has no accepted point")?;
        if *target != captured.ghostdag_data.selected_parent || parent_point.block != *target {
            return Err("tokenizer parent point differs from selected parent".into());
        }
        let at = PalwBlockContextV2 {
            block: kaspa_consensus_core::blockhash::ORIGIN,
            daa_score: captured.daa_score,
            blue_score: captured.ghostdag_data.blue_score,
            subsidy: self.coinbase_manager.calc_block_subsidy(captured.daa_score),
        };
        let extras = self.palw_transition_extras_for(&at).into_legacy_extras();
        let value = read(parent, params, &at, &extras)?;
        let parent_root = prepared.authenticated_root_v1().unwrap_or_else(|| parent.state_root());
        let current = self.virtual_stores.read();
        let after = current.state.get().map_err(|e| e.to_string())?;
        if !Arc::ptr_eq(&captured, &after) {
            return Err("tokenizer selected view changed; retry the read".into());
        }
        self.check_prepared_template_parent_v1(&prepared).map_err(|e| format!("tokenizer import/state changed: {e:?}"))?;
        Ok(Some(TokenizerRegistrationReadSnapshotV1 {
            parent_point,
            parent_root,
            quote_daa: at.daa_score,
            quote_blue_score: at.blue_score,
            value,
        }))
    }

    pub(crate) fn tokenizer_registration_open_quote_read_v1(
        &self,
        open: TokenizerRegistrationOpenBodyV1,
    ) -> Result<Option<TokenizerRegistrationReadSnapshotV1<Result<TokenizerRegistrationOpenQuoteReadV1, String>>>, String> {
        self.tokenizer_registration_read_v1(|parent, params, at, extras| {
            match kaspa_consensus_core::palw_state_v2::palw_tokenizer_registration_open_quote_v1(parent, params, at, extras, &open) {
                Ok(Some(quote)) => Ok(Ok(quote)),
                Ok(None) => Err("tokenizer quote rules became unavailable".into()),
                Err(error @ (PalwStateV2Error::NativeResource(_) | PalwStateV2Error::NativeContextUnavailable(_))) => {
                    Err(error.to_string())
                }
                Err(refusal) => Ok(Err(refusal.to_string())),
            }
        })
    }

    pub(crate) fn tokenizer_registration_status_read_v1(
        &self,
        open_id: Hash64,
    ) -> Result<Option<TokenizerRegistrationReadSnapshotV1<Option<NativeRetainedV1<TokenizerRegistrationStatusReadV1>>>>, String> {
        self.tokenizer_registration_read_v1(|parent, _, _, _| {
            kaspa_consensus_core::palw_state_v2::palw_tokenizer_registration_status_v1(parent, open_id).map_err(|e| e.to_string())
        })
    }
}
