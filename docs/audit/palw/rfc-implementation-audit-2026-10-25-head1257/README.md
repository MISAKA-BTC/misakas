# RFC1〜38 最新網羅監査 — HEAD 1257f0ae8（2026-10-25）

**受入まで完了は2/38本（RFC21・36）。コード項目充足は4/38本（RFC17・19・21・36）。** 前回からRFC単位の完了件数は不変。局所実装・試験の進展を全体完了へ加算していない。

実装先 `codex/rfc06-12-continuation` の確定HEAD **`1257f0ae8b2f33f5421e09be416f3aa86af0b47c`** を監査した。前回は `0ec2efc4c3ead4d2b63c994d4744c59545a75626`。元checkoutの別branchや公開稼働binaryと同一だという主張ではない。未コミットLegacy court実装は実行待ちのため除外した。

## 完了件数

| 区分 | 本数 | RFC |
|---|---:|---|
| 実装・受入確認済み | 2 | 21・36 |
| コード項目充足・受入未完 | 2 | 17・19 |
| 部分実装／全体未確認 | 30 | 01・02・03・06・07・08・09・10・11・12・13・14・16・18・20・22・23・24・25・26・28・29・30・31・32・33・34・35・37・38 |
| 延期・留保 | 2 | 04・15 |
| 合意runtime未実装 | 1 | 27 |
| 不採用・対象外 | 1 | 05 |

合計38本。RFC21は文書統治、RFC36は宣言された純ライブラリ＋休眠fenceの限定範囲。公開network全機能の運用完成を意味しない。RFC17・19は承認済み入力、実release/年次upgrade受入などが残る。

## 全38本の現在判定

| RFC | コード | 受入 | 決定的残件／範囲 |
|---|---|---|---|
| 01 | PARTIAL | PARTIAL | 実process全経路とrelease公開物の同一世代受入を一括立証する記録が不足。各機能の存在から全体完了へ推論しない。 |
| 02 | PARTIAL | PARTIAL | 共通derived resource契約はcurrent Source/局所courtで進んだが、全native graph/legacy callback/input/DB/cacheのconstruction-to-last-use予約は未閉鎖。 |
| 03 | PARTIAL | PARTIAL | 延期対象が共通fence/別入口から開かないことの現行release拒否matrixが未確認。Noneだけでruntime無効を認定しない。 |
| 04 | DEFERRED | DEFERRED | 当面の機能無効化を全entryで検証したrelease受入が未確認。positive改善実装は延期範囲なので現行完了のため追加要求しない。 |
| 05 | SUPERSEDED | NOT_APPLICABLE | BVM/GVM不採用。実装対象外であり、完了数へ加えない。 |
| 06 | PARTIAL | PARTIAL | 旧残件だったpre-object payout/EVM key arraysとcapacity sweepは閉鎖済み。現在の具体的残件はLegacy court_ingress_admission_v1→paid/legacy capのinflight/reserved Vec、first-side Setとfee入力cloneのcaller事前予約。未コミット修正はruntime PENDINGで除外。 |
| 07 | PARTIAL | PARTIAL | approved independent source/bootstrap inputと全stage direct-check/costの実acceptanceは未完成。normal refusal自体をバグ/未実装と数えない。 |
| 08 | PARTIAL | PARTIAL | 全reward/payout/material consumerと実provider replacement/restart/reorg/pruneの総合経路は局所検証を越えて未閉鎖。予約の残件はRFC6と共有。 |
| 09 | PARTIAL | PARTIAL | 一般post-checkpoint L2自主検証とprovider discoveryは現sourceで未提供。trusted checkpointの限定経路/UNVERIFIED拒否は完成している。 |
| 10 | PARTIAL | PARTIAL | source independence/bootstrap approvalとfleet無しの実全life-cycle受入が未完。None自体ではなく入力・source成立と実acceptanceの不足。 |
| 11 | PARTIAL | PARTIAL | K3正式入力未作成と実weight/context/fidelity/cohort90%受入未成立。検証機構が存在してもこのacceptanceを完了にしない。 |
| 12 | PARTIAL | PARTIAL | native39正式settlement写像が未策定のためactual native39→settlement consumerは閉鎖できない。仕様を創作せず設計決定を要する。 |
| 13 | PARTIAL | PARTIAL | TEST: §7の実Llama/全commit・同一schedule parity・全stage interruption/資源枯渇/accepted registration-reorgを一つの現候補で確認した証拠が不足。 |
| 14 | PARTIAL | PARTIAL | CODE: 公開source/typed preparationは接続済だが、全class/fault/回答/host-memory/cost consumerを包含した非Panel外部1人の完結性は未閉鎖。 |
| 15 | DEFERRED | MISSING | DEFERRED: Panel=0の現行有効化禁止は設計上の状態。外す作業を実装残件として提案しない。 |
| 16 | PARTIAL | PARTIAL | CODE: report容量/不足field固定None。 |
| 17 | COMPLETE | PARTIAL | TEST/EXTERNAL: κ/対象catalog editionの承認済recordとRFC19 release/実node annual受入は未提供/未完。コードの意図したrefusalを実装欠落としない。 |
| 18 | PARTIAL | PARTIAL | TEST: 本RFCの実process最上位T0f/T1e/T3c/T3d、認証再実行費用/grinding実測の包括記録不足。 |
| 19 | COMPLETE | PARTIAL | TEST/EXTERNAL: fixed old/new binaries・catalog/release signatures・実gateway/worker/artifact・R1–R8実node複数roleの一貫受入不足。 |
| 20 | PARTIAL | PARTIAL | CODE/DEFERRED: §6.3 KV履歴diskspillは仕様のみ。現在32Kで必要性が示されていない条件付き作業として区別。 |
| 21 | COMPLETE | VERIFIED | 文書統治・手続き・保存検査の対象範囲で完了。 |
| 22 | PARTIAL | MISSING | TEST: §3 runtime拒否/共通経路迂回不能/再起動再同期の対象release証拠不足。 |
| 23 | PARTIAL | PARTIAL | CODE: ChainClaim中心・inputs空のbuilder。全native/EXEC transaction/material/provenance依存閉包とclaim-local frontier/native lifecycle横断未完。 |
| 24 | PARTIAL | PARTIAL | TEST/DESIGN: 全対応形状/ctx2M/fault routeで完全witness/rounds/byte/work/IO/RAMとoriginal起点doorの閉鎖証拠不足。 |
| 25 | PARTIAL | MISSING | CODE/DESIGN: 既存court容量管理はあるが、一般v2 resource-vector予約calendar、bounded混雑extension/全依存terms原子更新/全native checkpoint closureが未閉鎖。 |
| 26 | PARTIAL | PARTIAL | CODE/POLICY: service profile未pin、production funded schedule未提供、全semantic/parameter/roleを含む全分岐予約が閉じていない。 |
| 27 | ABSENT | PARTIAL | CODE: 専用AccountID/元本lot/新責任・料金・RPC・原子admission・新報酬台帳は未実装。 |
| 28 | PARTIAL | PARTIAL | CODE/POLICY: preparation/default/non-ORIGINAL全dutiesの完全な因果graph・packet対応と原子的全branch予約/費用契約は未閉鎖。 |
| 29 | PARTIAL | PARTIAL | CODE/POLICY: OriginalAggregateDraft::reservation_readinessはUnresolvedSupplyContractを必ず返す。native allbranch packet/funding/semantic supplier契約なし。 |
| 30 | PARTIAL | PARTIAL | CODE/POLICY: 全終了grammar・全合法branchの原資/役割契約/finite保持・資本compositionの完全な導出がない。 |
| 31 | PARTIAL | PARTIAL | CODE/POLICY: 元期限内で全最遅branchが完了できる全graph/費用・予約契約は未閉鎖（§3–4の別条件）。 |
| 32 | PARTIAL | PARTIAL | CODE/POLICY: storage price/payer/TTL・実資金/全branchmaterial供給契約が未完成。Semantic tablesはmaterialtarget対象外でborrowed供給に留まる。 |
| 33 | PARTIAL | PARTIAL | CODE: 新feeの実徴収/返還・3MSKslashは未接続。全draw branchのroot受付前capacity/funding/retention原子予約が未完。 |
| 34 | PARTIAL | PARTIAL | CODE/POLICY: 公開有料coordinatorの全branch/material/料金・実原資・金融作業の契約は未完。 |
| 35 | PARTIAL | PARTIAL | CODE/POLICY: productionfundingschedule常時Err、fee/capital/retention/settlement/profile・role契約未提供。paidsettlementResolverNone。 |
| 36 | COMPLETE | VERIFIED_WITHIN_DECLARED_SCOPE | 純ライブラリ＋休眠fenceの宣言範囲で完了。node/backend運用完成は含めない。 |
| 37 | PARTIAL | PARTIAL | CODE: 全collectionserve/codec/typedassembly/wholeimport integration・NativeDemand接続が未完。newwire protocol/resume未接続。 |
| 38 | PARTIAL | PARTIAL | CODE: actualwinningcarrierindexなし。plainworker context/nativehandshake、liveTFP1/TFO1 producer、SDK/workerと他gateway出力経路の接続が未完。 |

## 前回後の実装進展

RFC06のpre-object payout/EVM key buffers、court capacity sweepを閉鎖し、対応する1要件群をPARTIALからCOMPLETEへ訂正。active court admission/session handoffと4種類のaux sweepも予約を追加した。全consumer・Legacy・全モデル費用境界は未完。

RFC07のprovider失敗座標を実checkerのChallenge選択へ接続、RFC08のprivate ACKを実queue consumerへ接続。全stage・実adapter4種・独立source・全reward/material lifecycleの受入は未完。

RFC12のimmutable selected parent、RPC/head retry、blocking session取消し、実native roundのgas capとempty/full execution整合を修正・検証済み。round gas修正は合意に影響し、旧binaryが30Mを受理した実REDがある。公開履歴との互換性・跨version replayは未確認であり、公開配備の完了には含めない。

## 網羅性と証拠確認

3担当が全38本・141要件群（52＋52＋37）を再照合。全主RFC本文は前回から同一。前回264引用ファイルは258同一・6更新で、追加実装と証拠を含む303ファイルの確定HEAD SHAを保存した。

前回354コード引用は336が同じ行、14が行移動、4が抽出・置換後の実consumerに対応することを記録した。引用の存在確認だけで完了判定せず、担当ごとの要件・caller・残件判断を併記した。

最新試験記録は固定binary6本のSHAと18完了groupのログを再確認し、子process・重複を除く158テストPASS、失敗0・ignored0を確認。今回全MUSTを実nodeで再実行した宣言ではない。RFC21の16件、RFC36の27件は前回受入と同一sourceの範囲で維持する。異なる世代の102/143/158テスト件数は合算しない。

K3正式入力は未作成、native39→RFC12正式決済写像は未策定。実work複数process・100k・OS crash・Latest/Safe/Finalized・bridge/market fill・履歴署名等を含むcampaignの15gateはUNKNOWN。公開activationはNone、通常selected-source/V6 paid bootstrap拒否、未承認economics休眠を維持。禁止された計算証明・旧モデル審査は実装残件として復活させない。

## 記録と再確認

- [RFC01〜12詳細](rfc01-12.md) / [JSON](rfc01-12.json)
- [RFC13〜25詳細](rfc13-25.md) / [JSON](rfc13-25.json)
- [RFC26〜38詳細](rfc26-38.md) / [JSON](rfc26-38.json)
- [件数](summary.json) / [全38本・141要件のcoverage](coverage.json)
- [確定source SHA](source-manifest.json) / [引用の現在位置](citation-resolution.json) / [既存runtime証拠の再検証](runtime-revalidation.json)
- [前回監査](../rfc-implementation-audit-2026-10-25/README.md)

```sh
python3 docs/audit/palw/rfc-implementation-audit-2026-10-25-head1257/verify_snapshot.py
python3 docs/maintenance/check_docs.py
```

機械検査は件数・参照・source同一性の検査であり、protocol正しさの証明ではない。
