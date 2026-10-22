"""Pinned reviewed handwritten Borsh codec bodies for RFC0006 shape generation.
Source hashes are review-change detectors, not executable acceptance evidence.
`reviewed-field-layout` retains field order/representation while the real decoder
may enforce stricter bounded/semantic subsets. Allocation proof never supplies
semantic authority. Adapters spell codecs with version tails/variable lanes.
"""
PINNED = {
# Claude 2026-10-22: shared request intent: derived order; signature length exactly ML-DSA-87.
'crate::palw_court_shared_tir_frontier_v1::SignedSharedRequestV1': ('reviewed-field-layout','c76f9fe8b93578271a6c2f148f69f24fc02db910960bdb2afb99a2f312bd4703'),
# Claude 2026-10-22: tag 178 shared op: derived enum framing (u8 variant, fields in order, u32-length vecs); every bound checked before allocation.
'crate::palw_court_shared_op_carriage_v1::PalwSharedCourtOpV1': ('reviewed-field-layout','0fafb0403000d4d65a42bf393e8b21512317a4b552e8143e622d15cedec98f8d'),
# Claude 2026-10-22: numerical part: derived order; version!=1 and empty/oversize bytes refused.
'crate::palw_court_numerical_transport_v1::NumericalPartV1': ('reviewed-field-layout','a09dcf897bfeba8adee0da6315f5ccb4c4ba054b7e22fbc2223f16dc312c580d'),
# Claude 2026-10-22: numerical manifest: derived order; decoder rejects version!=1 only.
'crate::palw_court_numerical_transport_v1::NumericalManifestV1': ('reviewed-field-layout','2f0c22e8f59e71fc823b26b9a23e724c32b9314637db8d19bf3440f3bf75e164'),
# Claude 2026-10-22: D2 readiness path: derived order (u32, Hash64, Vec<Hash64>); decoder bounds sibling count to MAX_PATH.
'crate::palw_court_model_scope_routes_v1::readiness_root::ReadinessDigestPathV1': ('reviewed-field-layout','defb98a6ba03ef7a581ec429f587ad00603ac97fda1cec1cdeabd19968624a05'),
# Claude 2026-10-22: upstream ScriptPublicKey = (u16 version, Vec<u8> script) in derived order; reached via funded-original pairing.
'crate::tx::script_public_key::ScriptPublicKey': ('reviewed-field-layout','b09c259eb179559998c7f9db8a51f40dc7afd81c0b2b5851f126ccba479b7f4b'),
# Claude 2026-10-22: TransactionMass = one u64 (AtomicU64 wrapper), reached via the funded-original pairing's carrier in extras.
'crate::tx::TransactionMass': ('reviewed-field-layout','65e95a8b99dee19d4612d720ab0fa64e28ef24c2f571a0c862690dbb22773151'),
# Claude 2026-10-22: TFO1 derived order (version, bytes, consent, bytes); decoder bounds both byte fields, checks version/structure.
'crate::palw_tir_funded_original_envelope_v1::PalwTirFundedOriginalEnvelopeV1': ('reviewed-field-layout','ab92c70ec6b555e0ba099174e87f63c4d654a48799c7d95869a7ef28628597ba'),
# Claude 2026-10-22: same field order as derived serialize; only bytes decode bounded to PART (read_bounded_bytes_v1).
'crate::palw_court_shared_material_intake_v1::MaterialPartV1': ('reviewed-field-layout','53e3c9266ea706f9362179965fb9fe26dce3e3a07f9ba1cccb2fb996b36c451a'),
# Claude 2026-10-22: derived serialize (terms, u32 len, bytes); decoder only adds terms check and exact ML-DSA-87 signature length.
'crate::palw_court_original_escrow_v1::FundingConsentV1': ('reviewed-field-layout','f35702cb4cbba930432c475f4287c0d497d767561b9effe3961a7e47484917d5'),
# G14 snapshot rows: decoder consumes derived field order; version, exact ML-DSA
# key/signature and path bounds narrow acceptance without changing field framing.
'crate::palw_court_shared_tir_root_v1::SharedTirRootRowV1': ('reviewed-field-layout','54395a01de88dee1dc2ffbe105ba68a8621fd16cdedc2a827e79e3d8ff18a9de'),
'crate::palw_court_shared_tir_round_v1::SharedTirRoundRowV1': ('reviewed-field-layout','908103e1fe58fe35610bf054fb7f5a93fbfc806bbcac006bccc7c35c580b31b4'),
'crate::palw_court_shared_tir_bottom_v1::SharedTirBottomRowV1': ('reviewed-field-layout','8d4ff323e6fa4dd713582c7301b903185fbc78512721a340101a840d9eca5c27'),
# Same field order as derived serialize; only bytes Vec decode is narrowed to existing PART.
'crate::palw_court_shared_tir_stream_state_v1::SharedTirChunkV1': ('reviewed-field-layout','5252de7400c7c9e3ed9a231f3286f9429dea9d1f652e24aebff67a5026fd1dd8'),
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
# Claude 2026-10-22: + funded-escrow tail 0xFB (unique among 74 tails), written only when non-empty; empty == old bytes.
# Claude 2026-10-22 (2): + D2 readiness tail 0xE5 (non-empty only; 75 tails unique).
'crate::palw_state_v2::PalwStateCarriageV2': ('adapter','03e35c21cf15a35cbed661234e09ce87b18c50dfcce67ad4b9dbd3a4798780c1'),
'crate::palw_tir_prefix_owned_v1::PalwTirPrefixLeafAnswerV1': ('reviewed-field-layout','1aa6332539dd6d430025fdd6d15e1737c9ccd0fad6ced34bc4798e4dcbfad422'),
'crate::palw_tir_public_owned_v1::PalwTirPublicChunkRecordV1': ('reviewed-field-layout','394604fa55e5ec254a05265c967a3604f9584f90505e54083bcd28b91b626d92'),
'crate::palw_tir_public_owned_v1::PalwTirPublicContractV1': ('reviewed-field-layout','a37709c03faa51fcc923e9055aa29a280ab0b09ef3cfa086ef66997762cf745f'),
'crate::palw_tir_public_owned_v1::PalwTirPublicPendingV1': ('reviewed-field-layout','1ac3396c3c5a2de5699b5c0164d480eaf099cb0efeca354745932bb865b30342'),
'crate::palw_tir_public_owned_v1::PalwTirPublicSlotV1': ('reviewed-field-layout','dc905c91c97914082760b784060e472a1593f5142e388d7981d313f582ff4b47'),
'crate::palw_tir_public_owned_v1::PalwTirPublicWorkDebitV1': ('reviewed-field-layout','ed4276f62badba8b2d69155d2f7fdbb869d1fa2659127130be140e74566a65c3'),
'crate::palw_tir_public_owned_v1::PalwTirPublicWorkRulesV1': ('reviewed-field-layout','8ad95ed209aa5dfe8d5609986bf48de7cc50399de66c13f18ef568fcae839a0e'),
'crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryActionV1': ('reviewed-field-layout','3241221855be580bc370417c34216a4a962717b207a7bbe43d837234203e25cf'),
'crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryOfferV1': ('reviewed-field-layout','947037069405875fde30a7cde95075645efe7b496138a9599e10f5a95a1d4642'),
'crate::palw_work_slice_v2::PalwExecOriginWitnessV1': ('reviewed-field-layout','5017f37eacd00a129c5d75862110ab97186f3b5d7660507493d98d2a7715f2f7'),
'misaka_palw_kernel::merkle3::LeafOpeningV3': ('adapter','582e62f4dc91bf880a6b4d7a26844983e16d81e9a146e632492181d4250a70ba'),
'misaka_palw_panel::PanelCandidatesV1': ('adapter','31fc1462f95406b67fd1c21d5612daf979d83d9508b1794ef419121b3b1c5bcc'),
# G14 original TIR route (2026-10-15, integ/g14-remaining): each body read by hand against its struct/enum declaration —
# field order and representation are the derive layout (u8 enum tags in declaration order, u32-prefixed collections,
# Option as u8 0/1); the decoders only add pre-allocation bounds, canonical-order and duplicate refusals.
'crate::palw_tir_public_carriage_v1::PalwTirPublicControlV1': ('reviewed-field-layout','ac20f0083098873053009bd778b7fb149bea9f2de21f03cd52e700fb6a9c77cd'),
'crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1': ('reviewed-field-layout','3ce1e73405110241f03f316b92c5d9da2b1ee26cb51146c1fbb6789605826caf'),
'crate::palw_tir_public_carriage_v1::PalwTirPublicWitnessPartV1': ('reviewed-field-layout','796acf4ed2ff5e5ca12f92f29b13d75280372aaa42f013352a10a43222af0738'),
'crate::palw_tir_public_carriage_v1::PalwTirPublicGrantReadyV1': ('reviewed-field-layout','d5c9aa2a7cea6997fbfc37a6af280e3cf704007009b44aafd2d5cf96ebb7e2dd'),
'crate::palw_tir_public_custody_v1::PalwTirPublicCustodyContractV1': ('reviewed-field-layout','248d1220c6fbd16bc325312303d7270ed82ec76f0852200fdb3f4a5a2b4b641f'),
'crate::palw_tir_public_custody_v1::PalwTirPublicCustodyGrantV1': ('reviewed-field-layout','2ca7a248a7694cb8b1b6efff219412ac3b93cfe7c35e64c37791e7016dd098df'),
'crate::palw_tir_public_custody_v1::PalwTirPublicCustodyPledgeV1': ('reviewed-field-layout','14b4b9db923bd2424e65a22f6f81665f7d0f96cfc28fc0110bc6be3f15044c44'),
'crate::palw_tir_public_custody_v1::PalwTirPublicOriginalCustodyV1': ('reviewed-field-layout','18672a9a3ad9e55b4e96e473276a673e52496c6ab2590048b81988a6a52a1ce0'),
'crate::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1': ('reviewed-field-layout','27800a9840ef4792cccbe7e22cf06f59c0cdda094f23ed517c074b5d068374dd'),
'crate::palw_tir_public_owned_v1::PalwTirPublicChunkRecordV1': ('reviewed-field-layout','394604fa55e5ec254a05265c967a3604f9584f90505e54083bcd28b91b626d92'),
'crate::palw_tir_public_owned_v1::PalwTirPublicContractV1': ('reviewed-field-layout','a37709c03faa51fcc923e9055aa29a280ab0b09ef3cfa086ef66997762cf745f'),
'crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1': ('reviewed-field-layout','3466d969363509459096937bf38dc0ccc8c2c61ef4c3e28bef83585417e726cf'),
'crate::palw_tir_public_owned_v1::PalwTirPublicPendingV1': ('reviewed-field-layout','1ac3396c3c5a2de5699b5c0164d480eaf099cb0efeca354745932bb865b30342'),
'crate::palw_tir_public_owned_v1::PalwTirPublicSlotV1': ('reviewed-field-layout','dc905c91c97914082760b784060e472a1593f5142e388d7981d313f582ff4b47'),
'crate::palw_tir_public_owned_v1::PalwTirPublicWorkDebitV1': ('reviewed-field-layout','ed4276f62badba8b2d69155d2f7fdbb869d1fa2659127130be140e74566a65c3'),
'crate::palw_tir_public_owned_v1::PalwTirPublicWorkRulesV1': ('reviewed-field-layout','8ad95ed209aa5dfe8d5609986bf48de7cc50399de66c13f18ef568fcae839a0e'),
}
