# source/numerical以外のcanonical inventory足場（read-only、未実装）

対象: 現integration候補。nativeの有限geometryとstate-transition grammarを列挙する。値/証拠bytes/将来DAA/producer rootを偽造しない。以下はprofile価格・全original予約・供給・資金成立を示さない。

## 実入口

- frame自身の公開裁定: `palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1` は Arithmetic / Output / DecodeToken の3形。native受付は `palw_tir_public_fold_v1::apply_control_v1`→`apply_conviction_v1`。後者はactual frame・現在filer bond・収録point・producer/Valid signerの保持holdと実徴収を確認する。証拠上限はproducer prepaid witness ceiling。Holds/Need/EOF短縮は有罪にも正当性証明にもならず拒否。
- semantic: `palw_staged_preclaim_v1::staged_preclaim_tir_quote_v1` と `palw_tir_public_allocation_v1::derive_tir_public_allocation_v1` がactual登録semantics/inventoryから必要familyを選ぶ。`frame.plan.semantic.stages` のfamilyは PublicPrefix / Constraint / Tokenizer（最大3）。`frame_semantic_bundle_v1`→`install_original_v1` がClaimBundleと元producer history pledgeを接続する。
- semanticの実move入口: `palw_constraint_stream_carriage_v1::PalwConstraintStreamMoveV1/ActionV1`→`palw_staged_court_fold_v1`。family外variantを一律合法として列挙してはいけない。旧規則の時計やabsence collectを承認F02へ自動採用しない。

## 各branchの列挙式・依存

### Output / stop

`PalwTirPublicConvictionEvidenceV1::Output { rows_root, ids }`→`adjudicate_original_output_v1`→`original_stop_rule_fault_v1`。原本のdecode count Dに対する完成ID列1本。証拠payloadはtag + 64B rows_root + 4B count + 4D（外側署名/claim/scope/stream/chunk overheadは別にactual Borshで数える）。source7familyのcanonical FullIds materialを共有できる。Ids各chunkを再送する別owner専有streamを新設しない。

stopはoriginal job ruleで決まる。Keyed/V3はbudget長、V4は`decode_answer_stop_v4(config, decode_token_limit, vocab, ids)`。値依存のstop reasonをoriginal受付時に決めず、同一materialに対するterminal guard/outcomeとして保持する。`OutputHolds`でclaim信用を作らない。

### Selection / logits / non-dissected arithmetic

`PalwTirSelectionRuleV1::of_job_v1(original.job)`でKeyed/V4選択を固定。constrained jobはframe単独selection doorでは拒否しConstraint stageへ進む。`palw_tir_public_selection_probe_v1`は実行時のcommitted/replay ID差分から最大2 TraceTileを要求するが、そのhonest単一差分だけではcanonical全分岐ではない。

値非依存の予約候補は各row `0..D` × legal tile `0..ceil(V/L)` の取得（既存source graphと共有）と、各positionのselection terminal intent。`TirDecodeTokenTiled`証拠はcomplete IDs＋row opening＋committed/beat tile openingsを持つ。全token-pairを別の資金streamへ展開せず、role/position/target-kindとnative最大payloadを持つ終端1枠にできるか既存verifierのexact read-set/grammarを確認する必要がある。V4 sampler等が追加materialを要求する場合は不足をErrにする。

Arithmeticは`PalwCourtVerdictProofV2::{TirCone,TirLogits}`。non-dissection cone rootsは全native leafを走査し`palw_tir_dissect_site_v1==None`と分類するだけでは十分でなく、state/hist/logits leaf種別ごとの既存builder admissibilityを保持する。Logitsはnative output-coordinate universeから列挙。parameter/prompt潜在coverと共有source materialを接続し、全leafを『numerical bottomだけ』へ押し込めない。サイズは`court.max_close_bytes`/original witness ceilingと実enum envelopeで検査し、上限を増やして収めない。

### Constraint

既存action grammar: EogPart/EogFinish、RenderingPart/RenderingFinish、EogTokenFinish、NativeSelectedDemand/NativeSelectedAnswer/SelectedEventCollect、stage activate/collect等。TIR原本はFlat/Float source variantsをそのままlegalにしない。

`PalwSelectedEventCoordinateV1 {row,tile}`→NativeSelectedDemand。有限候補はD×ceil(V/L)、rows/IDs/TraceTileの認証依存はsource graphと共有可能。`PalwSelectedEventSessionV1::{demand_view_v1,answer_event_view_v1,collect_v1}`がactualnext request/current signers/deadlineを判断する。既存最大moves/resource_planはenvelopeであり全request iteratorではない。

EOG/Renderingはcanonical offset + bounded parts + finish/seal。実lengthが短ければ未使用tail partsを無応答義務にしない。生成ID/EOGの値・rendering parser stateにより次moveが変わるので、logical DAGはpotential-positionとguardを分離する。サイズ/part maximaは既存decoder constantsとsigned planから導出し、実bodyなしで勝手な平均lengthを使わない。

### Tokenizer

前提inventory: `PalwTokenizerArtifactInventoryOfferV1`のproducer署名→ArtifactInventoryOpen→`PalwTokenizerArtifactInventoryActionV1::{Part{offset,bytes},Finish}`。既存 `quote_v1(maximum_bytes,config)` と `PALW_ARTIFACT_INVENTORY_PART_BYTES_V1`から全part枠＋finishを導出する。raw byte長・key-event bound・登録tokenizer identityを別々に照合。

裁定: `PalwTokenizerStreamPacketV1 {offset,bytes,proofs}`→TokenizerSeal→TokenizerRenderingPart等。`PalwTokenizerTableStreamV1`はsigned raw length/key bound、operand raw length、JSON parser、rendering digest streamを持つ。単純なbytes/PARTだけでは全key proof/parser/event処理・保持を覆わない。native `PalwTokenizerResourcePlanV1::derive_with_key_bound`/bundle inventory planで有限上界はあるが、canonical全parser stage/compact proof costをgraphに戻すiteratorは未接続。

### PrefixState / PublicPrefix

TIR原本: `PalwOwnedTirPrefixCourtV1` + `PalwTirPrefixLeafAnswerV1`、NativeTirPrefixAnswer/NativeTirPrefixCollect。元native boundaryの`total_v1()`座標全体、各native signer slot、各leafのopening/byte ceilingからpotential response枠を列挙できる。`tir_prefix_resource_plan_v1(total_leaves,leaf_bytes,depth,maximum_custodians,prices)`はexisting bound。ここでslotsはmaximum_custodians+ownerであり、source7familyのmaxcustodians（producerを含む）と同じ数え方とは限らない。勝手に+1を追加/削除せず、`slot_ordinal_v1/has_response_duty_v1`と合わせる。

legacy prefix family: `PalwPrefixCourtActionV1`のBeginTile/TilePart/FinishTile、PublicRow、BeginHead/BeginPagedHead、HeadPageRead/Write/Digest、HeadInputOpening/Part/Finish等。`PalwPrefixFamilyCourtV1::cursor_v1()/next_public_source_v1()`がcanonical次unitを返す。TIRに不適用なGDN/headpage grammarを混ぜない。`PalwPrefixPublicAcquisitionPlanV1::derive_v1`は各source demand + signer Begin/parts/Finishを価格化するが、data-dependent next cursorを実bodyなしにどう全閉包化するかは別のchecked walkが必要。

## 認証/actor/依存

- original producer / Valid custodians / active filer / relay carrierを分離。第三者relayがbytesを運んでも元署名者が変わらない。input signerや現在ownerをcourt signerだと推定しない。
- frame bundleのresource_plan_id / original source / registered semantics / tokenizer inventory / history pledge/grantの署名scopeとmaterial horizonを照合。`ActivateStage`のprevious/next seedは元bundleの固定family順に束縛。
- `HistoryCustodyGrant/HistoryRecovery`、SelectedEventCollect、PrefixPublicCollectはfull observer-independent traversalに必要。過去owner/state machineの進捗を未来ownerへ引き継げるかはgrammar上のmove数だけでは証明できない。
- 全canonical geometryの認証IDはsource＋original claim＋native rules/profile＋family/role/coordinate/stage、absolute受付scopeは別にbind。producerの将来root/bytes、honest実行readset、caller complete flagをcanonical completenessに使わない。

## 終了効果（数の訂正を維持）

RFC0030 §3は6終了理由＋完成assembly不整合の7行。八つはarchive試験数でgrammar cardinalityではない。native verdict enumはExecutorGuilty/ChallengerDefeatedの2形、shared bottom outcomeはNativeFault/NoMismatchFoundの2形。これらとdefault/Panel放棄/service障害/hard到達を同一terminal判定へ畳まない。

既存 `palw_court_end_archive_v1::{PendingCourtArchiveV1,CourtEndEvidenceV1,CourtEndDispositionV1}`、close receipt、archive verify/dischargeはnative終了証拠とpending責任の保存/再検査足場。全state grammarに対して field別効果（claim credit、active slot、確定/未決責任、担保hold、資料、frontier、pruning）を列挙するadapterは未完。archive保存額は実escrow・徴収・返還ではない。F02 defaultには実供給と配送/収録前提、障害にはno-slash＋保持/復旧guardが必要。hard到達だけで無罪/返還/有罪を作らない。

## 次の検証単位

実原本prepare→actual semantic stage順/geometry→family別potential transition inventoryをnative版のenumとして列挙し、native decoder/action validatorが受理する各variant/coordinateがinventoryに入ることをcross-checkする。Resource upperとのcount/byte比較、未対応variant/予算不足の全Err、短bodytail、非適用family拒否、actor/署名scope違いを試験する。これを既存native admission testから呼ぶが、価格・fullgrammar・UTXO escrowが揃うまではfunded original admissionを拒否し続ける。新helperを追加しただけで完成とは扱わない。
