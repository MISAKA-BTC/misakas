"""Pinned reviewed handwritten Borsh codec bodies for RFC0006 shape generation.
Source hashes are review-change detectors, not executable acceptance evidence.
`reviewed-field-layout` retains field order/representation while the real decoder
may enforce stricter bounded/semantic subsets. Allocation proof never supplies
semantic authority. Adapters spell codecs with version tails/variable lanes.
"""
PINNED = {
'crate::palw_bond_budget_v1::PalwBondBudgetStateV1': ('adapter','163e6501bd1b84fdf6f3ac7eff4cf62cee3495a3658c9047bcba633ac5d86abf'),
'crate::palw_flat_constraint_stream_v1::PalwFlatConstraintActionV1': ('reviewed-field-layout','82eac4b65c1f7faae83e691c850e3e3cc5529e4c331f88bbc4d17797f3b89a33'),
'crate::palw_float_constraint_source_v1::PalwFloatSourceActionV1': ('reviewed-field-layout','ef5bac2471e454620a0b6b70050b62ccfe2dbcca9139824197d04892588459af'),
'crate::palw_fp_job_v5::PalwFreePromptJobV5': ('adapter','a17756257533823c05db4806b0650e73b4eea9d756125549b5aaf84cbc34939a'),
'crate::palw_freeprompt_v3::PalwFreePromptJobV3': ('adapter','78ed7366585736ecf9d14d1cafa72014d59835726d25c16cf25aa60ee723a338'),
'crate::palw_history_custody_v1::PalwHistoryCustodyGrantV1': ('reviewed-field-layout','5778cad63684d88cedc5583fb4ff3e3e2c2ea2b6ffe2235b6241c6588deca9b3'),
'crate::palw_history_custody_v1::PalwHistoryCustodyPledgeV1': ('reviewed-field-layout','3d78da63351e37e640542629432b2f8a4b531d7cd4991c8a5e5512d25104aa6a'),
'crate::palw_history_custody_v1::PalwHistoryRecoveryActionV1': ('reviewed-field-layout','2b188a73ad325402fb1448169cecc58fea6ec0e1fecb809ab08488ac8c517ffb'),
'crate::palw_history_custody_v1::PalwProducerCustodyDelegationV1': ('reviewed-field-layout','96cc753a800a00371f35876dc747c76077e6ec6bdfabce4ae49bc3956fcdb8a0'),
'crate::palw_model_market_v1::PalwModelMarketV1': ('adapter','7d3ee61081f0441271f482fabd883ded41c21f718042a21458b83bd3e5604c29'),
'crate::palw_onboarding_tile_v3::ArtifactTileOpeningV3': ('adapter','2944c31fc3a536af0588e66f381024812b6e0db46dd95a29c656cec1e53ad06e'),
'crate::palw_pipeline_da_v1::PalwReservedObjectV1': ('reviewed-field-layout','c441eac1b37001bea2b1697a92462e3be55dfaa66be1887eb0d8b54ff683af15'),
'crate::palw_public_binding_v1::PalwPublicLegacyBindingHeaderV1': ('reviewed-field-layout','900095e7188817e0c96bf4ad9bf0d3bcc55e6b88b3b4ac82987c94e8d51f20dd'),
'crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1': ('reviewed-field-layout','83f5acb15f134a6704d499cb0d461ff319ec015335c8688a56f20c7462a8a9f9'),
'crate::palw_selected_event_source_v1::PalwSelectedTiledEventV1': ('reviewed-field-layout','7f8237e529545c68327c9ee0cbc74d5f916eeb2413db12936088f7e20fc4c191'),
'crate::palw_staged_bundle_v1::PalwStagedBundlePlanV1': ('reviewed-field-layout','049c32133b7944f49670cbffd8ad581e47bd0e7f8cc3369b969fb93969026178'),
'crate::palw_staged_bundle_v1::PalwStagedFamilyCheckpointV1': ('reviewed-field-layout','734f45329f56538ee85d85977e841f357de05634b6b37b846622feaea926780b'),
'crate::palw_staged_bundle_v1::PalwStagedFamilyRowV1': ('reviewed-field-layout','b92b6935c58487df4bdf9f5e254db9a43e018a57463827d3cf49b1bc0c01270a'),
'crate::palw_state_v2::PalwStateCarriageV2': ('adapter','7344dcc14d16436e54235966fa07280d52a191a9091f258b33f467b07c2b933b'),
'crate::palw_tir_prefix_owned_v1::PalwTirPrefixLeafAnswerV1': ('reviewed-field-layout','1aa6332539dd6d430025fdd6d15e1737c9ccd0fad6ced34bc4798e4dcbfad422'),
'crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryActionV1': ('reviewed-field-layout','3241221855be580bc370417c34216a4a962717b207a7bbe43d837234203e25cf'),
'crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryOfferV1': ('reviewed-field-layout','947037069405875fde30a7cde95075645efe7b496138a9599e10f5a95a1d4642'),
'crate::palw_work_slice_v2::PalwExecOriginWitnessV1': ('reviewed-field-layout','5017f37eacd00a129c5d75862110ab97186f3b5d7660507493d98d2a7715f2f7'),
'misaka_palw_kernel::merkle3::LeafOpeningV3': ('adapter','582e62f4dc91bf880a6b4d7a26844983e16d81e9a146e632492181d4250a70ba'),
'misaka_palw_panel::PanelCandidatesV1': ('adapter','31fc1462f95406b67fd1c21d5612daf979d83d9508b1794ef419121b3b1c5bcc'),
}
