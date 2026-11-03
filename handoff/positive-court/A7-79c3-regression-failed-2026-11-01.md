# A7 — 79c3固定候補の独立回帰（2026-11-01）

判定: **不合格**。production base `79c3f9bcae0fde86940fd65cbcbfa2018a72cc21`、A7 runner追加後の検査対象 `7dd56d8cd1e31b684a97e35c6650eff1b04fab80`。production/Rust/Cargo/lockはbaseと同一で、Root currentを参照せずA7自身のtreeからビルドした。

正式runnerはNode会計の6失敗で `FAILED_OR_INCOMPLETE` として停止した。残り6コマンドは同じsource/環境で別の診断記録として実行し、`DIAGNOSTIC_ONLY_FORMAL_CANDIDATE_FAILED` を保持した。全12枠では661成功・6失敗・ignore worker 1件。成功件数は試験呼出し数で、同fixtureの複数driver実行を含む。

| 検査 | 成功 | 失敗 |
|---|---:|---:|
| Python guard/config/process | 26 | 0 |
| A7 independent/pipeline | 20 | 0 |
| 共通契約 | 51 | 0 |
| 会計leaf | 53 | 0 |
| Node会計 | 49 | **6** |
| Miner | 61 | 0 |
| Panel/source | 52 | 0 |
| TIR/Node/Panel/Court | 139 | 0 |
| core integration（fence8/source37/lifecycle3/旧TIR9） | 57 | 0 |
| core Positive unit | 126 | 0 |
| 旧G14 shared | 27 | 0 |

別途no-runのbuild成功を記録したが、実行件数には入れていない。pipelineと専用fenceは各8件の期待名すべてが実際に成功した。ignore workerは親試験から9つの実プロセスとして明示実行した。

## 6件の失敗原因

すべてA5 `misaka/palw/positive-accounting-node/src/rows/tests.rs:216` のDTO state importで止まり、後段のA5 reader/creditには到達していない。新core strict restoreに対し、旧正常fixtureにJournalHead、Pending対応SourceUnion、attempt使用量、異なる受理位置、cursor10に対応する10 PacketReceiptが不足・不整合だった。不正rowをimport成功させて後段で拒否する旧negative期待も、早期拒否する新restoreと衝突した。

productionの検査を緩める根拠は見つからず、A0/A5へfixtureと拒否段階の期待の更新を依頼した。A5修正 `7463c2f69` が提示されたため、原本を保持して別candidate・別prefixで正式12コマンドを再実行する。owner側55件や本候補の他の成功を、その新候補の成功件数へ足さない。

## 証拠・実証範囲

[正式manifest](../../testing/positive-court-a7/evidence/a0-79c3-regression-2026-11-01/run.json)、[失敗原ログ](../../testing/positive-court-a7/evidence/a0-79c3-regression-2026-11-01/05.log)、[残り診断](../../testing/positive-court-a7/evidence/a0-79c3-regression-2026-11-01/remaining-diagnostic/diagnostic.json)、[最終整合確認](../../testing/positive-court-a7/evidence/a0-79c3-regression-2026-11-01/failure-integrity-verification.json)を保存した。独立担当もsource集合5,169件、両lock、native archive、42実行binary、各ログ・group終了・9role残留なしを照合した。最終identity errorsは空。失敗Node binaryはignored保存領域へコピーしhashを保持した。

既存shared cacheを使用し、新しい巨大target/C++ buildを作成していない。互換native archiveをpath/hash/environmentで固定し、debug0/incremental0/coreopt0/jobs2、`RUST_MIN_STACK=16777216` を記録した。default stackでの合格とは扱わない。容量確保は旧A7 cacheの未使用rlib/rmeta6件・1,425,801,140 bytesだけ。source/lock/rawを変更せず、過去の保護対象25 executableの個別hashも再照合した。

coreのvocab8193 coverageはunoptimizedな旧fixtureの生成費用が大きく、1秒のread-only stack sampleは `parts` → `open_artifact_leaf_v1` / Merkle構築を示した。計時を補正していない。この観測を実LLMやMiner追加負担率へ換算しない。

新fullrelationの再導出・元dtype・完全binding・read set・終端coverageも静的レビューしたが、A7独立scalar oracleの小規模算術と、ownerの広い合成fixtureは異なる実証範囲である。unsigned/missing deliveryの反復でlocal prototypeの予算を使い切れる可用性制約は残り、native ingress完成とは扱わない。

実LLM、native176 writer/新job授権/actual native Env、実ネットワーク、DB reopen/IBDを認定していない。小さい別プロセス測定のA2 Minerはfixtureで、実A2接続は別pipeline。source fee/refundと成功worker報酬を分け、private権利oracleからnative資格を生成しない。公開有効化・model tierは変更なし。
