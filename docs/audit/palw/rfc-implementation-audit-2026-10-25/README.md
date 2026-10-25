# RFC1〜38 最新実装・受入監査 — 2026-10-25

監査した実装基点は **`0ec2efc4c3ead4d2b63c994d4744c59545a75626`**。全38主文書を、現行原則・後続改定・実コード・実caller・検証記録と照合した。38本を対象とした監査であり、全MUSTを実ノードで新たに実行した宣言ではない。未確認は完了に加えない。

**コード面の実装項目が揃ったものは4本（RFC17・19・21・36）、実装と該当受入まで確認できたものは2本（RFC21・36）。** RFC21は文書統治、RFC36は宣言済みの純ライブラリ＋休眠fenceの範囲である。この2本から公開network・mainnet・全機能の運用完成を推論しない。

## 件数

| 区分 | 本数 | RFC |
|---|---:|---|
| 実装・受入確認済み | 2 | 21・36 |
| コード項目は充足・受入未完 | 2 | 17・19 |
| 部分実装／全体未確認 | 30 | 01・02・03・06・07・08・09・10・11・12・13・14・16・18・20・22・23・24・25・26・28・29・30・31・32・33・34・35・37・38 |
| 延期・留保 | 2 | 04・15 |
| 合意runtime未実装 | 1 | 27 |
| 不採用・実装対象外 | 1 | 05 |

合計38本。コード項目充足4本は上の先頭2区分の合計。RFC17/19は承認済み入力・実release/年次upgrade受入等が残り、最終完了には含めない。RFC03は画像・動画の延期と共通部分を分け、共有経路・無効化の受入が未確認のため部分実装に置いた。RFC04は延期、RFC15はPanel=0 modeの延期・留保。RFC05はVM不採用の決定であり、VM実装完了にも新たな実装義務にも数えない。RFC27は提案・測定資料があるが専用合意runtimeがない。

## 判定方法

- コードの存在、実caller接続、検証・受入、有効化を別の軸で確認する。`None`や空のarming recordだけをコード欠落とは判定しない。
- 現行適用条項と明示的な延期・廃止・将来別RFC範囲を区別する。旧モデル審査・禁止されたモデル計算証明を未実装残件として復活させない。
- helper/library/fixtureのPASSを実node、実モデル、全consumer、独立source、全campaignのPASSへ広げない。古い「未接続」記述は現在のcallerと照合し、接続済みコードを未実装扱いしない。
- 具体的な反例・未接続・固定None/Err・必須受入の欠如を記録し、不確かな項目は未確認のままにする。複数RFCが同じ未完成の契約に依存するので、未完RFC数は独立した実装タスク数ではない。

## 全38本

| RFC | コード判定 | 受入判定 | 最終区分 | 主な範囲・残件 |
|---|---|---|---|---|
| [0001](rfc01-12.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | 実process全経路とrelease公開物の同一世代受入を一括立証する記録が不足。各機能の存在から全体完了へ推論しない。 |
| [0002](rfc01-12.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | 共通derived resource契約はcurrent Source/局所courtで進んだが、全native graph/legacy callback/input/DB/cacheのconstruction-to-last-use予約は未閉鎖。 |
| [0003](rfc01-12.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | 延期対象が共通fence/別入口から開かないことの現行release拒否matrixが未確認。Noneだけでruntime無効を認定しない。 |
| [0004](rfc01-12.md) | DEFERRED | DEFERRED | 延期・留保 | 当面の機能無効化を全entryで検証したrelease受入が未確認。positive改善実装は延期範囲なので現行完了のため追加要求しない。 |
| [0005](rfc01-12.md) | SUPERSEDED | NOT_APPLICABLE | 不採用・実装対象外 | BVM/GVMを実装しない現行決定。versioned kernelとdeclarative planを採用する参照文書。 |
| [0006](rfc01-12.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | baseline pre-object queue Vecとcourt capacity ended/clone/pools/grantsはwriter予約より前の実確保を残す。current uncommitted waveは含めない。 |
| [0007](rfc01-12.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | approved independent source/bootstrap inputと全stage direct-check/costの実acceptanceは未完成。normal refusal自体をバグ/未実装と数えない。 |
| [0008](rfc01-12.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | 全reward/payout/material consumerと実provider replacement/restart/reorg/pruneの総合経路は局所検証を越えて未閉鎖。予約の残件はRFC6と共有。 |
| [0009](rfc01-12.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | 一般post-checkpoint L2自主検証とprovider discoveryは現sourceで未提供。trusted checkpointの限定経路/UNVERIFIED拒否は完成している。 |
| [0010](rfc01-12.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | source independence/bootstrap approvalとfleet無しの実全life-cycle受入が未完。None自体ではなく入力・source成立と実acceptanceの不足。 |
| [0011](rfc01-12.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | K3正式入力未作成と実weight/context/fidelity/cohort90%受入未成立。検証機構が存在してもこのacceptanceを完了にしない。 |
| [0012](rfc01-12.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | native39正式settlement写像が未策定のためactual native39→settlement consumerは閉鎖できない。仕様を創作せず設計決定を要する。 |
| [0013](rfc13-25.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | TEST: §7の実Llama/全commit・同一schedule parity・全stage interruption/資源枯渇/accepted registration-reorgを一つの現候補で確認した証拠が不足。 |
| [0014](rfc13-25.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | CODE: 公開source/typed preparationは接続済だが、全class/fault/回答/host-memory/cost consumerを包含した非Panel外部1人の完結性は未閉鎖。 |
| [0015](rfc13-25.md) | DEFERRED | MISSING | 延期・留保 | DEFERRED: Panel=0の現行有効化禁止は設計上の状態。外す作業を実装残件として提案しない。 |
| [0016](rfc13-25.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | CODE: report容量/不足field固定None。 |
| [0017](rfc13-25.md) | COMPLETE | PARTIAL | コード項目は充足・受入未完 | TEST/EXTERNAL: κ/対象catalog editionの承認済recordとRFC19 release/実node annual受入は未提供/未完。コードの意図したrefusalを実装欠落としない。 |
| [0018](rfc13-25.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | TEST: 本RFCの実process最上位T0f/T1e/T3c/T3d、認証再実行費用/grinding実測の包括記録不足。 |
| [0019](rfc13-25.md) | COMPLETE | PARTIAL | コード項目は充足・受入未完 | TEST/EXTERNAL: fixed old/new binaries・catalog/release signatures・実gateway/worker/artifact・R1–R8実node複数roleの一貫受入不足。 |
| [0020](rfc13-25.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | CODE/DEFERRED: §6.3 KV履歴diskspillは仕様のみ。現在32Kで必要性が示されていない条件付き作業として区別。 |
| [0021](rfc13-25.md) | COMPLETE | VERIFIED | 実装・受入確認済み | 文書・統治手続き・検査ツールの範囲。 |
| [0022](rfc13-25.md) | PARTIAL | MISSING | 部分実装／全体未確認 | TEST: §3 runtime拒否/共通経路迂回不能/再起動再同期の対象release証拠不足。 |
| [0023](rfc13-25.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | CODE: ChainClaim中心・inputs空のbuilder。全native/EXEC transaction/material/provenance依存閉包とclaim-local frontier/native lifecycle横断未完。 |
| [0024](rfc13-25.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | TEST/DESIGN: 全対応形状/ctx2M/fault routeで完全witness/rounds/byte/work/IO/RAMとoriginal起点doorの閉鎖証拠不足。 |
| [0025](rfc13-25.md) | PARTIAL | MISSING | 部分実装／全体未確認 | CODE/DESIGN: 既存court容量管理はあるが、一般v2 resource-vector予約calendar、bounded混雑extension/全依存terms原子更新/全native checkpoint closureが未閉鎖。 |
| [0026](rfc26-38.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | CODE/POLICY: service profile未pin、production funded schedule未提供、全semantic/parameter/roleを含む全分岐予約が閉じていない。 |
| [0027](rfc26-38.md) | ABSENT | PARTIAL | 合意runtime未実装 | CODE: 専用AccountID/元本lot/新責任・料金・RPC・原子admission・新報酬台帳は未実装。 |
| [0028](rfc26-38.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | CODE/POLICY: preparation/default/non-ORIGINAL全dutiesの完全な因果graph・packet対応と原子的全branch予約/費用契約は未閉鎖。 |
| [0029](rfc26-38.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | CODE/POLICY: OriginalAggregateDraft::reservation_readinessはUnresolvedSupplyContractを必ず返す。native allbranch packet/funding/semantic supplier契約なし。 |
| [0030](rfc26-38.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | CODE/POLICY: 全終了grammar・全合法branchの原資/役割契約/finite保持・資本compositionの完全な導出がない。 |
| [0031](rfc26-38.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | CODE/POLICY: 元期限内で全最遅branchが完了できる全graph/費用・予約契約は未閉鎖（§3–4の別条件）。 |
| [0032](rfc26-38.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | CODE/POLICY: storage price/payer/TTL・実資金/全branchmaterial供給契約が未完成。Semantic tablesはmaterialtarget対象外でborrowed供給に留まる。 |
| [0033](rfc26-38.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | CODE: 新feeの実徴収/返還・3MSKslashは未接続。全draw branchのroot受付前capacity/funding/retention原子予約が未完。 |
| [0034](rfc26-38.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | CODE/POLICY: 公開有料coordinatorの全branch/material/料金・実原資・金融作業の契約は未完。 |
| [0035](rfc26-38.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | CODE/POLICY: productionfundingschedule常時Err、fee/capital/retention/settlement/profile・role契約未提供。paidsettlementResolverNone。 |
| [0036](rfc26-38.md) | COMPLETE | VERIFIED_WITHIN_DECLARED_SCOPE | 実装・受入確認済み | 純ライブラリ＋休眠fenceの明記範囲。node運用/backend移行は含めない。 |
| [0037](rfc26-38.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | CODE: 全collectionserve/codec/typedassembly/wholeimport integration・NativeDemand接続が未完。newwire protocol/resume未接続。 |
| [0038](rfc26-38.md) | PARTIAL | PARTIAL | 部分実装／全体未確認 | CODE: actualwinningcarrierindexなし。plainworker context/nativehandshake、liveTFP1/TFO1 producer、SDK/workerと他gateway出力経路の接続が未完。 |

## 優先して閉じる共通残件

1. **全branchの供給・費用・資本・保持契約**（RFC26/28〜35など）。native lifecycle、notice、coordinator、archiveは存在するが、production funded schedule、実原資・役割契約、aggregate予約の閉鎖が残る。
2. **実資金への接続**。RFC33の新fee/slash、RFC35の精算resolver、RFC34の公開archived terminal ingressなどは、型やprivate fixtureだけでは完了しない。
3. **依存関係・容量の実consumer**。RFC23のcheckpoint builderは全namespaceのinput閉鎖が残り、RFC16のreport容量/不足量は実sourceでNoneのまま。RFC06/08の全予約・reward/material consumerも局所検証を越える残件がある。
4. **本物の受入入力とcampaign**。K3正式入力は未作成、native39→RFC12決済写像は未策定。実work複数process・100k・OS crash・Safe/Finalized・bridge/market fill・履歴署名、年次upgradeの実release/実artifact受入は未完。入力や成功証拠を創作しない。
5. **IBD全経路と無効化確認**。RFC37の有界stream/visitorは全collection codec・typed組立/import・generation/storage/wire/予算を全面接続したものではない。延期された画像・動画・モデル改善は、別入口・共有経路・restart/syncを含む無効化の確認が別途必要。

## 今回の実行・証拠確認

- baselineの固定Core executableでRFC36対象 **27/27 PASS**：auth map19、row access7、state adapter1。純ライブラリの反例拒否・root/pin/更新・cache/Unavailableを対象とし、公開node統合を主張しない。
- RFC21の文書ツール **16/16 PASS**。原Markdown **660/660 SHA一致**、manifestの全 **2481移動先が存在**。古いorganized hashと後日の正当な編集を混同しない。
- baseline build parentから監査基点までの変更Rust11pathが、実際の固定binary生成時のSHAと一致することを再確認した。[基点同一性](baseline-verification.json) / [実行結果](runtime/results.json) / [原本保存検査](runtime/rfc21-original-archive.json)。
- 全38本を **141要件群**で照合。**354件のコード引用**の行・symbol・既記載SHAを基点と突合し、264ファイルのSHA manifestを保存した。見出し/規範marker抽出はナビゲーションであり、語数を意味の検証に代用しない。

### 未コミット作業の扱い

監査中はproduction/test sourceを固定した。court capacity sweep、exact selected-parent API、checked head notificationは未コミットなので、この完了数には入れない。最初の4package buildはPASSしたが、新native parentテストは3/4で、storage fixtureのGhostdagCompact level prefixの修正待ち。他の新7テストのPASSをこの未完1件やRFC全体の完了へ読み替えない。既存dirty作業は保持し、監査のために削除・commitへ混入していない。

公開activationは **None**、通常selected-source/V6 paid bootstrapは拒否、未承認economicsは休眠、実campaignの15gateは **UNKNOWN** のまま。

## 再確認・詳細

```sh
python3 docs/audit/palw/rfc-implementation-audit-2026-10-25/verify_audit.py
python3 docs/maintenance/check_docs.py
```

[件数JSON](summary.json) / [全要件統合JSON](requirements.json) / [基点SHA manifest](source-manifest.json) / [38本文の索引](primary-inventory.json)。引用行は監査基点の行であり、未コミット作業や後続変更でcurrent fileの表示行がずれる場合は指定commitの内容を読む。

- [RFC01〜12 詳細](rfc01-12.md) / [機械記録](rfc01-12.json)
- [RFC13〜25 詳細](rfc13-25.md) / [機械記録](rfc13-25.json)
- [RFC26〜38 詳細](rfc26-38.md) / [機械記録](rfc26-38.json)

最終機械検査: 38本の一意性・件数・354コード引用・264 baselineファイルSHAはPASS。[集計/引用検査結果](runtime/audit-validation.json)。文書1226件の検査は既知43・新規0、strict全正常とは区別する。[文書検査結果](runtime/docs-validation.json)。監査前後で実装5変更ファイルと全38本文のSHA不変を確認した。
