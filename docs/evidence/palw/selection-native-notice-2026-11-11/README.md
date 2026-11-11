# Selection署名と受理済みcaseの照合

状態: **IMPLEMENTATION_IN_PROGRESS / Rust build・test NOT_RUN / 公開有効化なし**。

目的はStudioから提出する同一TFP1の受付・認証・Finalを通し、当選時にそのclaimの権利消費とブロック生成を確認することである。本記録はその前提となるSelection資料要求の認証部分に限る。旧profileの成功やローカルfixtureを公開達成へ読み替えない。

## 実装範囲

- 固定幅の内側`SignedSelectionNoticeV3`を追加。network・claim・Source・branch・logical case・physical reservation・position・beat・filerを別domainのML-DSA署名に含め、短い/余剰wireと不正versionを拒否する。外側搬送tagはまだ接続していない。
- 実paired TFP1の保存リンクと受理済みdraw/manifest requestから導いたcaseに、noticeの全識別情報を照合する。native bondが存在するだけでなく、その鍵のprincipalが受理済みdrawのprincipalと一致することを要求する。
- 認証結果は署名・branch・Source・stateを借用する一時値とし、署名で選んだ行からcanonicalな資料rangeを導く。供給者別の物理ordinal、range ID・長さ・認証方式を照合する。同一range IDでも別供給者の予約番号を流用しない。
- 既存native link/case fixtureに実鍵署名の回帰コードを追加。未登録bond、鍵principalの差し替え、別の有効行、範囲外row/beat、別case/reservation/branch/Source/filer/network、別role ordinal、欠損/差し替えrangeを対象にする。fixtureのbondはsnapshot入力へ明示追加しており、有料登録や実送信を検証したものではない。

関連ソース:

- [notice codec・認証・資料要求解決](../../../../consensus/core/src/palw_court_shared_material_intake_v1/batch/selection/notice.rs)
- [受付前branchのcanonical range照合](../../../../consensus/core/src/palw_court_shared_material_intake_v1/batch/admission/plain_selection.rs)
- [受理済みdraw principal](../../../../consensus/core/src/palw_positive_contract_v2/protected_batch_case.rs)
- [native case回帰コード](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original.rs)

## 接続上の残件

この認証値はaccepted noticeでも支払済みservice権限でもない。writerは実行中prefixからcaseを再導出し、standing・実費・割当DAA・rankごとの一回性・保存とrollbackを検査する必要がある。

現在の[共通Batch予約](../../../../consensus/core/src/palw_batch_court_v1.rs)は元notice期限の翌DAA以降にOpen等を配置する。そのOpenを期限内Selection noticeの代替にしない。先行ManifestRequestとSelection noticeの同時受理、または期限前の因果関係を持つ予約と受理を、実費・共同checkpointとともに接続することが次の実装箇所である。全量モデル配信やHF取得可否の裁定は追加しない。

その後もSelection応答のnative writer、terminal/回復・archive、完全TFO3 coordinator、公開quote/intake、Panel承認・Final、当選時権利消費とブロック生成の実測が必要。既存の拒否gateを外してこの部分実装を公開有効化していない。

## 最後に保存された公開観測

前の作業中に行ったread-only照会を、`public-readback.jsonl`と`binary.txt`へ原文保存した。今回のコードで再照会・送信・配備した結果ではない。

- 対象: testnet-12、`169.58.232.113`のseat7、JSON websocket `127.0.0.1:26354`。
- 元claim: `7645bc8bbba0e00a35ccbac77ecb0e28d489688161bd0130649f3575919a83df225e509936a52c4b0e30a415e71d50da1f38dd87a1f02ce4267bc145fc6a3711`。
- 観測DAA: **21239**。`available=true`、`quoted=false`、`hasPlan=false`、拒否は **G14_ROUTE_ABSENT(FpTokenizer)**。
- 稼働binary: `151f9ce7b/kaspad`。SHA-256は`binary.txt`に保存。

この観測は受付・Final・権利消費・生成ブロックの証拠ではない。`validation.json`はRust実行なしと文書検査の既存エラーを区別する。[元実装計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)に従い、実装途中の小刻みなCargo実行は行っていない。
