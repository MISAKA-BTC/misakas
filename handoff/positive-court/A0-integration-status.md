# A0 integration checkpoint

2026-11-01更新。workspace `/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet`、branch `codex/positive-court-a0`。RFC-0044 / ADR-0212の実装調整。A1〜A7への直接連絡はユーザーが明示許可した。元checkoutや各担当のdirty作業を一括コピーしない。

| 担当 | A0へ統合した範囲 | 未完了・保留 |
| --- | --- | --- |
| A0 | v2/r1共通型、frame作者認証、非循環funding scope、176/151/P、固定row `327393e8e`、無効のtyped fence、P所見 `069d54007`、record `d284f5902`、source read interface `49c86968c` | native live event・writer、実資金lock、P実draw、全consumer・復旧との結合 |
| A1 | exact checker、PreparedCheck/NodeChecker、signed ingressと費用計上、pure coreへ一元移植 `5ef211508 / 9cd63d984`、実験用expanded geometry（新wire/derivationなし） | 実LLMの未commit i64/i128 laneを検査する別derivation/body、native期待値と資源予約 |
| A2 | 初期r1、bound通常推論・file provider・durable有限原資料応答 `e2c2d13f0`＋保護prefix修正 `56f5332ba` | 未保護tailの反例を独立監査し修正版として受理。native保護store/dutyと同一candidateの最終再検証は別 |
| A3 | 実PanelBuilder、r1 witness/frame、G14 cold資料、A1連結、非循環funding、primitive record接続 `12f243d1f / 829a0da3c`。別候補148件の証拠 `43858d820` | 最新統合候補の再検証、native受付・実LLM測定 |
| A4 | d204固定のowned Court/coordinator/body0..7を `815b7ca1c` へ統合。candidate事前認可・accepted frame無課金ACK、narrow prototype `d028330f8`、future frame課金前拒否 `17990b173` | actual176 one-writerとFull seal、canonical origin/job/role/profile、追加future-frame修正のruntime検証 |
| A5 | phase3、typed row reader、source due、3勘定spend DATA計算、共有due型aliasを `d3d4054a5 / c596a4106 / 312df466b / 456999953` へ統合 | actual lockと全consumer、跨settlement一回払い・未払due/refund条件、P実承認 |
| A6 | native aux151 scanner/heap/構造restore、順序/counter、direct decode降順拒否、共有journal/連鎖と実A1 local再生 `0d678c8be` | production NativeReplayEnvironment、実source/funds/P、typed material store・同batch/reorg/IBD。旧private archiveをaliasしない |
| A7 | signed pipeline/runner/原証拠を `3bb1c1e16`、A4再監査・future-copy反例を `c75ebcbfe` へ選択統合 | 歴史的243件を現A0成功へ流用しない。A2/A4修正後の固定candidate実行が必要 |

共通契約はrow段階29件、P所見後35件、record後41件とClippy成功を別snapshotに保存した。core/test sourceとconsensus libraryの型検査、fence専用8件も成功。各担当の別候補での成功と区別する。frame署名は作者の認証、FundingConsent署名は支払条件への同意だけである。row/cursor保存やP所見認証がnative権限を生成することはない。新Pは全LLMで実抽選の`Certified`のみを認め、`PublicCheckCertified`・旧S/K/V・local replay単独を流用しない。

native全経路・実UTXO精算・全consumer・複数Node/reorg/IBD/pruning・実LLM cold/warm測定は未完了。公開presetの有効化は行わない。実装全体の完了や認定profileの成立を宣言しない。

直近のA0実行では、A6 preflight追加後の共通契約42件（旧説明44件を訂正）とClippyが成功（対象source安定）。A4統合直後のcore型検査は4エラーで失敗し、missing opaque identity比較と未統合wide参照を修正した後、`d028330f8` のcore library型検査はsource安定で成功した。これは後続patchやnative実行の成功を含まない。A7 process/manifest Python8件も成功した。詳細は [A0検証記録](../../docs/design/palw/positive-court/VALIDATION-A0.md)。

全Caseの新native履歴は [journal契約](../../docs/design/palw/positive-court/NATIVE-JOURNAL-R1.md) に固定した。完全なlogical no-opはCase課金・履歴append前に判別し、実state/cost-changing失敗は保持する。actual origin/headerではfees/Panel metadataによる抽選再試行、origin計算rootとCourtの厳密結合、旧経済条件との整合が未解決。[origin annex](../../docs/design/palw/integration/positive-native-attempt-origin-r1.md) の非許可境界を維持する。


`0d678c8be`までの追加統合: A2のsession seal・raw reopen時の全meter復元・logical NoOp/CASと実remote接続、A4のFull保持・RAM lease必須化・readonly Court recovery receipt、A5の用途別entitlementと第8用途WorkerPayment、A6の共有journal codec/連鎖と実A1検査器によるlocal replay foundation、A7の8用途独立fixtureを含む。actual native SourceRead/roles/funding/one-writer/P/全consumer/IBDは未接続である。

同候補の共通51件・private会計53件とClippyは成功した。旧共通44件の説明は42件へ訂正し、原ログは保持した。A1 source-only full-relationは`d657007ce`、A3 one-pass typed captureは`da941bb55`、両canonical tokenizer修正は`4bf984f35 / dbe95bacc`へ統合した。A2 source journal bridgeは`a87e7f789`へ統合した。typed→full-relation adapterとactual native環境は未接続。別候補の件数を合算しない。

最初のaccepted native assignment経路はレビュー済みの新RFC/ADR案`08b0db8ba`への明示採択を照会中。旧PAV2/PAV3・L1・発行規則の置換を許可する案ではない。採択前に新authority/wireを実装しない。


`79c3f9bca`でsource候補を固定: A1のimmutable metadata/plan/checker borrow API、A2の意味不変lint cleanup、A6のcaller-batch journal store・actual accepted-data readerと独立レビューを統合した。先行`dbe95bacc`のsourceではcanonical/full-relation/r1/typed32件＋source専用37件がsource安定で成功。古いlib filterの0件実行は合格に数えず別原ログを保持。現候補のconsensus library型検査はexit0・全対象source不変で成功。A7全対象rerunは同sourceで行い、別候補証拠を加算しない。

A7へ`79c3f9bca`固定sourceでの独立回帰開始を依頼した。A6の実SourceExternalState追加候補`1b7fff400`とA3のtyped→FullRelation逐次adapterは後続レビュー・接続・同候補検証が必要。新nativeAssignment案の採択回答は未受領。
