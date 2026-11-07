# MISAKA RFC v2 — 41本を15本へ統合

入力: `misakas-pre(5).zip` / 整理日: 2026-10-29。

**普段読む・更新するRFC本文は15本。旧41本は既存パスのまま参照・履歴として保存する。**
この整理は文書構成の変更で、未採択案の採用、mainnet公開、コード完成、有効化を意味しない。

[旧41本→新15本の完全対応表](MAPPING.md) / [移行方法・未解決事項](MIGRATION.md) / [機械可読の節・行対応](source-map.json) / [検査結果](VALIDATION.md)

## 15本の構成

| 新RFC | 担当する範囲 | 主な旧RFC |
| --- | --- | --- |
| [01](0001-inference-and-generative-jobs.md) | 推論・生成Job・入出力 | 01・03 |
| [02](0002-tensor-ir-and-shared-query-data.md) | TIR・Kernel・共有Queryデータ | 02・29 |
| [03](0003-model-onboarding-and-artifacts.md) | モデル登録・変換・Artifact | 11・13 ＋02のonboarding節 |
| [04](0004-model-tiers-and-annual-catalog.md) | モデル二階層・年次カタログ | 17・19 |
| [05](0005-bounded-resources-and-long-context.md) | 資源上限・長文脈メモリ・計測 | 20 |
| [06](0006-panels-and-probabilistic-verification.md) | Panel参加・抽選・確率的検証・認証Receipt | 06・07・10 |
| [07](0007-claims-exec-and-answer-accounting.md) | Claim・EXEC・仕事量・回答会計 | 08・38 |
| [08](0008-claim-dependencies-and-liability.md) | Claim依存・Prefix生成元・未解決責任 | 23・40 |
| [09](0009-remote-client-and-sdk.md) | Remote Client・SDK・非保管型操作 | 09 |
| [10](0010-dag-consensus-and-native-settlement.md) | DAG合意・Gate C・Native EVM決済 | 12・18 |
| [11](0011-bond-budgets-fees-and-payments.md) | Bond予算・配分・Fee同意・決済会計 | 16・39 ＋15の共通会計 |
| [12](0012-authenticated-state-storage-and-ibd.md) | 認証State・保存・Pruning・IBD | 36・37 |
| [13](0013-permissionless-bootstrap-and-release.md) | 公開Bootstrap・Genesis・有効化とRelease接続 | 41 |
| [14](0014-court-master-contract.md) | Court統合契約 — 受付・対話・裁定・資金・責任終了 | 14・24・25・26・28・30・31・32・33・34・35 ＋他RFCのCourt専用条項 |
| [15](0015-freeze-deferred-rejected-and-governance.md) | 設計凍結・延期・不採用・未採択・変更管理 | 04・05・15・21・22・27 ＋03の画像・動画／撤回・未採択部分 |

## 最初に読む場所

LLMを使う経路は **01（job）→ 03（登録）→ 02（TIR実行）→ 06（Panel検査）→ 07（claim会計）→ 10（合意・決済）**。
不正・異議・無応答は **14（Court）**、入力再利用の影響は **08（依存）**、お金は **11（会計）**、同期は **12**、起動は **13**。
**何を凍結・延期・不採用としたかは15だけを入口にする。**

02がTIRと旧29の共有query/dataを所有し、14がCourtの手続き・責任を所有する。両方で同じprotocolを再定義しない。

## 状態を混ぜない

設計凍結は実装停止ではない。延期は永久不採用ではない。未採択は拒否済みではない。
旧15のPanel=0と、同じ文書内の採用済み共通bond会計を分離した。旧39も採択済みfeeと未採択の役務有限化A/Bを分離した。

各RFCは短い統合本文を先頭に置き、その後に担当する原条文を収録している。原条文の情報・数式・コードを捨てず、旧41本へ戻らなくても該当分野をまとめて読める。出典・元行番号・原文のhashと配置検査を付ける。原条文中の旧番号・§参照は旧版を指す。

## 番号を壊さない

新しい文書識別子は **RFC-v2-0001〜RFC-v2-0015**。旧RFC-0001〜0041とは別系列である。
旧番号をコード、wire tag、署名domain、fence名、履歴digestまで一括置換しない。文書を15本へ減らすことと、protocolを再採番することは別である。

通常の追記は対応する15本へ集約する。重大問題に伴う設計変更は、既存の凍結規程に従う新規RFC＋ADRを別の変更提案として扱い、採択後に15本へ反映する。

## 統合基準日後の追加文書

並行作業で追加された [ADR-0210](../adr/0210-all-model-g14-panel-one.md) / [RFC-0042](../rfc/0042-all-model-g14-panel-one.md) は、入力snapshotの204 ADR／41 RFCに含まれない追加文書として保持する。今回の50 ADR／15 RFCへ原文を混ぜず、採択・実装・有効化の状態は各文書の記録を参照する。`registry.json` の `post_consolidation_documents` からもたどれる。旧番号の意味は変えない。

2026-10-31に採択された学習済みLLMの出力表現の追加は [RFC-0043](../rfc/0043-wide-q24-llm-output.md) / [ADR-0211](../adr/0211-wide-q24-llm-output.md)。旧i32 Q24・既存schemeを保持し、別のi64 Q24 profileを追加する。公開有効化は別判断。入力原本へ混ぜず、post-consolidation文書として読む。

2026-10-31に実装指示されたPositive Court R2は [RFC-0044](../rfc/0044-positive-court-full-coverage.md) / [ADR-0212](../adr/0212-positive-court-without-model-prepossession.md)。主担当は14、演算・資料認証は02を共用する。[実装状態](../design/palw/positive-court/IMPLEMENTATION-STATUS.md)。原付録と旧0042／0210は保持し、公開網は未有効化。

2026-11-07の再検討は [RFC-0047](../rfc/0047-batch-court-time-contract.md)。上位は [公開裁定の方針 v0.1](../design/palw/consensus/public-adjudication-policy-v0.1.md) である。Batch の新しい版だけが時間契約を始める。Panel の署名は署名者の識別であり、定足数は評決ではない。合意ノードはモデル計算を端から端まで再実行しない。完了 receipt から 10,000 を始めない。306,108 DAA の局所化は代替経路ではない。G14 の batch 予算は測定したモデルではない。既存の 3,120 と 10,000 は延ばさない。15本へは混ぜない。P0–P3 には試験がある。debug の局所 door 27 は class の受理ではない。`PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1` は空である。出荷 preset は未武装。T12 の upgrade はなく、main へは push していない。
