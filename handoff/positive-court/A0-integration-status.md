# A0 integration checkpoint

2026-11-01更新。workspace `/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet`、branch `codex/positive-court-a0`。RFC-0044 / ADR-0212の実装調整。A1〜A7への直接連絡はユーザーが明示許可した。元checkoutや各担当のdirty作業を一括コピーしない。

| 担当 | A0へ統合した範囲 | 未完了・保留 |
| --- | --- | --- |
| A0 | v2/r1共通型、frame作者認証、非循環funding scope、176/151/P、固定row `327393e8e`、無効のtyped fence、P所見 `069d54007`、record `d284f5902`、source read interface `49c86968c` | native live event・writer、実資金lock、P実draw、全consumer・復旧との結合 |
| A1 | pure core exact検査器、署名/費用、ExpandedPlan・source-only FullRelation `d657007ce`、canonical tokenizerとborrow API | typed→FullRelation adapter、wide最終出力、native期待値と全資源予約、実LLM範囲/性能 |
| A2 | 通常推論・有限原資料応答、session seal、protected raw reopen meter復元、logical NoOp/fullCAS、common journal bridge `a87e7f789` | actual native duty/read factory、qualified paid receipt更新、実money owner |
| A3 | r1 Panel/record/G14 cold資料、one-pass typed capture `da941bb55`、canonical tokenizer `dbe95bacc` | 逐次typed→legacy opening/FullRelation adapter検証、実LLMの範囲/peak/性能 |
| A4 | Court/coordinator/body0..7、future-frame課金前拒否、actual Full carrier保持・RAM lease必須化、readonly復旧receipt | actual176 one-writer、canonical job/role/profile/schedule、実資金lockとP |
| A5 | 独立entitlement照合、旧7用途維持＋第8 WorkerPayment、first-source fee照合、readonly private会計receipt | 全native consumerと実coin lock/一回払い/安全返金・責任floor、P実承認 |
| A6 | aux151 scanner/restore、shared journal/実A1 local replay、caller-batch material store/actualdata reader `6e25d7631`、実SourceLedger/global差分/fee隔離 `3773083f9` | production NativeReplayEnvironment・actual roles/資金/lease、kind1 qualified spend、P2P/IBD/GC/T12 |
| A7 | 独立fixture/runner、future-copy反例、8用途fixture、A4 R5証拠監査。production79c3をA7自身のtreeへ独立固定 | guard準備後の同candidate全対象回帰、native/実LLM/複数Node性能。過去候補件数を合算しない |

共通契約はrow段階29件、P所見後35件、record後41件とClippy成功を別snapshotに保存した。core/test sourceとconsensus libraryの型検査、fence専用8件も成功。各担当の別候補での成功と区別する。frame署名は作者の認証、FundingConsent署名は支払条件への同意だけである。row/cursor保存やP所見認証がnative権限を生成することはない。新Pは全LLMで実抽選の`Certified`のみを認め、`PublicCheckCertified`・旧S/K/V・local replay単独を流用しない。

native全経路・実UTXO精算・全consumer・複数Node/reorg/IBD/pruning・実LLM cold/warm測定は未完了。公開presetの有効化は行わない。実装全体の完了や認定profileの成立を宣言しない。

直近のA0実行では、A6 preflight追加後の共通契約42件（旧説明44件を訂正）とClippyが成功（対象source安定）。A4統合直後のcore型検査は4エラーで失敗し、missing opaque identity比較と未統合wide参照を修正した後、`d028330f8` のcore library型検査はsource安定で成功した。これは後続patchやnative実行の成功を含まない。A7 process/manifest Python8件も成功した。詳細は [A0検証記録](../../docs/design/palw/positive-court/VALIDATION-A0.md)。

全Caseの新native履歴は [journal契約](../../docs/design/palw/positive-court/NATIVE-JOURNAL-R1.md) に固定した。完全なlogical no-opはCase課金・履歴append前に判別し、実state/cost-changing失敗は保持する。actual origin/headerではfees/Panel metadataによる抽選再試行、origin計算rootとCourtの厳密結合、旧経済条件との整合が未解決。[origin annex](../../docs/design/palw/integration/positive-native-attempt-origin-r1.md) の非許可境界を維持する。


`0d678c8be`までの追加統合: A2のsession seal・raw reopen時の全meter復元・logical NoOp/CASと実remote接続、A4のFull保持・RAM lease必須化・readonly Court recovery receipt、A5の用途別entitlementと第8用途WorkerPayment、A6の共有journal codec/連鎖と実A1検査器によるlocal replay foundation、A7の8用途独立fixtureを含む。actual native SourceRead/roles/funding/one-writer/P/全consumer/IBDは未接続である。

同候補の共通51件・private会計53件とClippyは成功した。旧共通44件の説明は42件へ訂正し、原ログは保持した。A1 source-only full-relationは`d657007ce`、A3 one-pass typed captureは`da941bb55`、両canonical tokenizer修正は`4bf984f35 / dbe95bacc`へ統合した。A2 source journal bridgeは`a87e7f789`へ統合した。typed→full-relation adapterとactual native環境は未接続。別候補の件数を合算しない。

最初のaccepted native assignment経路はレビュー済みの新RFC/ADR案`08b0db8ba`への明示採択を照会中。旧PAV2/PAV3・L1・発行規則の置換を許可する案ではない。採択前に新authority/wireを実装しない。


`79c3f9bca`でsource候補を固定: A1のimmutable metadata/plan/checker borrow API、A2の意味不変lint cleanup、A6のcaller-batch journal store・actual accepted-data readerと独立レビューを統合した。先行`dbe95bacc`のsourceではcanonical/full-relation/r1/typed32件＋source専用37件がsource安定で成功。古いlib filterの0件実行は合格に数えず別原ログを保持。現候補のconsensus library型検査はexit0・全対象source不変で成功。A7全対象rerunは同sourceで行い、別候補証拠を加算しない。

A7へ`79c3f9bca`固定sourceでの独立回帰開始を依頼した。A6の実SourceExternalState追加候補`1b7fff400`とA3のtyped→FullRelation逐次adapterは後続レビュー・接続・同候補検証が必要。新nativeAssignment案の採択回答は未受領。


後続candidate `1d6641471`: A6実SourceLedger/global差分をfee修正と失敗隔離試験を含めて統合。[独立レビュー](../../docs/evidence/positive-court-a0-2026-10-31/integration/a6-source-replay-readonly-review-20261101.json)。owner199の53件は[A6記録](../../docs/design/palw/positive-court/A6-SOURCE-REPLAY-HANDOFF.md)の候補だけを示し、Root79やphase1の38件に合算しない。wide構造認識`50339b43a`は新7＋既存golden14の21件と対象Clippyが成功。明示V2 logit primitivesを`82146b3b9`へ選択統合したが、その新5試験とprofile resolver/旧未対応入口ガードは次の検証範囲。


`b3da72953`まで: explicit wide data primitives・profile resolver・旧record/StepSpace/bindingの未対応wide拒否を統合。stage2はprofile11/source2/data5の18件が同sourceで成功、Core strict Clippyは等価style整理後に成功。A7の79正式runはA5旧fixture6件失敗の原本を保持、`7463c2f69`のtest-only修正をA0 `b3da72953`へ統合しA7再実行を依頼した。owner55件は16MiB stackの別候補。A3 adapter `01b5fed24`は同時保持予約の不足が見つかり保留し、persistent予約とlive materialの保守的合算を修正中。P/native actual job/roles/money/全consumer/復旧・IBDと実LLM性能は未完了。新assignment RFC/ADR案の明示採択は未受領。
