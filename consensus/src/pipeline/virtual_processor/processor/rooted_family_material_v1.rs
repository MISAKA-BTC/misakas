//! Native retained reads from the committed tip. Callers still file ordinary
//! signed court moves; material retrieval itself has no adjudication effect.
use super::*;
use kaspa_consensus_core::{
    palw_court_rooted_semantic_v1::{self as rooted, RootedSemanticTableV1},
    palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1, NativeRetainedV1},
    palw_state_v2::PalwChainStateV2,
};

impl VirtualStateProcessor {
    fn read_rooted_family_material_v1<T>(
        &self,
        read: impl FnOnce(&PalwChainStateV2) -> Result<Option<NativeRetainedV1<T>>, NativeDecodeErrorV1<String>>,
    ) -> Result<Option<NativeRetainedV1<T>>, NativeDecodeErrorV1<String>> {
        let Some(params) = self.palw_state_params_v2.as_ref() else { return Ok(None) };
        let store = self.palw_state_v2_store.read();
        // Keep the native state owner and the store's read lock through source,
        // inventory, horizon and byte authentication. A concurrent tip change
        // cannot mix the table of one original with another original's scope.
        let Some((_, state)) = store.load_tip_cached(params).map_err(|why| {
            NativeDecodeErrorV1::Resource(NativeCoverageV1::unsupported::<PalwChainStateV2>(
                "rooted family material tip",
                &why.to_string(),
            ))
        })?
        else {
            return Ok(None);
        };
        read(&state)
    }

    pub fn palw_rooted_family_table_v1_impl(
        &self,
        claim: Hash64,
    ) -> Result<Option<NativeRetainedV1<RootedSemanticTableV1>>, NativeDecodeErrorV1<String>> {
        self.read_rooted_family_material_v1(|state| rooted::palw_retain_rooted_family_table_v1(state, claim))
    }

    pub fn palw_rooted_tokenizer_raw_v1_impl(
        &self,
        claim: Hash64,
    ) -> Result<Option<NativeRetainedV1<Vec<u8>>>, NativeDecodeErrorV1<String>> {
        self.read_rooted_family_material_v1(|state| rooted::palw_retain_rooted_tokenizer_raw_v1(state, claim))
    }
}
