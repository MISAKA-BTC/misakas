# A7 — 固定候補の独立回帰完了（2026-11-01）

**正式12コマンドで667件成功・失敗0。** 対象は `c2e4226253900c3711ec2a4ee7d2ec8b548e57c5`、結果は `PASS_PINNED_PRIVATE_R1_CANDIDATE_ONLY`。成功は同じ候補の試験呼出し数で、同fixtureの複数driver実行を含む。既定ignoreのworker 1件は、親試験から9つの実OSプロセスとして明示実行した。

production baseはA0固定 `79c3f9bcae0fde86940fd65cbcbfa2018a72cc21`。Rust差分はA5 ready `7463c2f69` の `positive-accounting-node/src/rows/tests.rs` だけで、production/Cargo/coreはbaseと同一。既存A7 worktreeを独立branchへ切り、A0の後続Root sourceを参照せず検査した。

| 同一候補の検査 | 成功件数 |
|---|---:|
| Python guard/config/process | 26 |
| A7 independent / 実A2→A3→A1→A5 private pipeline | 20 |
| 共通契約・journal・codec | 51 |
| 会計leaf | 53 |
| Node会計 | 55 |
| Miner | 61 |
| Panel/source | 52 |
| TIR/Node/Panel/Court（15 targets） | 139 |
| core integration（fence8/source37/lifecycle3/旧TIR9） | 57 |
| core Positive unit | 126 |
| 旧G14 shared | 27 |
| 合計 | **667** |

no-runは両A7 binaryのbuild/hash確認だけで、成功件数に加えていない。pipeline・専用 `positive_court_fence_v2` は、それぞれ最低8件と期待8関数名の全成功を要求した。ignored、別target、部分summary、no-run、normal executionの欠落による見かけの成功を拒否する。最終missing requirementsとidentity errorsはいずれも空。

## 失敗の検出と修正確認

直前の79c3候補はNode会計の6試験でstrict restoreが旧fixtureを拒否した。[不合格報告](A7-79c3-regression-failed-2026-11-01.md)と生ログ・診断はそのまま保持し、PASSへ昇格していない。A0/A5へ原因を返し、fixtureだけを修正した後、本報告の正式12コマンドを新しいprefixで最初から実行した。過去の661/6、243、別A4候補の300、owner側55件を今回の667へ合算していない。

修正fixtureは基本6 rows＋JournalHead＋PacketReceipt10の17 rows、Journal sequence12、受理位置5→6→7..16、NotRequested、attempt1を満たす。不正ケースは実import拒否と原因照合へ移り、productionの検査を緩めていない。unsigned 300 bytesはheader284＋ExactPart16というDTO最低構造で、実A1 packet・算術・P・native資格の成功を表さない。正常DTO状態でも全8用途のnative OwnerMissing拒否を維持する。

## 接続・独立期待値

実bound A2 parameter認証/推論/署名 → A3 Panel builder → 同一core型のA1 opaque terminal → A5 `TirCreditView` を接続した。小さい4×4・3 decodeの全logitsとtokenは、TIR評価器とは別のscalar oracleで確認する。別class/params/job/input、全面的に自己整合する別scope packet、署名した偽算術・偽Miner出力、各leaf破損、不完全prefixを拒否する。Panel証拠拒否とMiner反証を分け、未検証を計算不正へ変換しない。

8用途の権利は共通Positive/P/資金条件から生成しない。既存primaryの明示oracleは旧7用途だけを許可しWorkerPaymentを拒否、paid-workerの別oracleはWorkerPaymentと適格Settlementだけを許可する。port未実装は全8用途を拒否し、paid認可の用途・完全bindingも照合する。これらはsynthetic private権利であり、既提供資料の料金・安全な返金には別のreceipt/liability/funds/once条件を維持する。

fullrelation11、expanded plan5、typed capture9等はwide/sparse・history・canonical tokenizer等の広い合成fixtureを実行する。共有TIR型/primitiveの前提があり、A7独立scalar oracleとは異なる範囲。unsigned/missing deliveryの反復でlocal prototypeの予算を消費できる既知の可用性制約も試験が明示しており、native ingress完成とは扱わない。

## 費用の一回観測

| warm 4×4 / 3 decode | elapsed秒 | process peak RSS bytes |
|---|---:|---:|
| baseline | 0.000441583 | 10,272,768 |
| Miner fixture | 0.019324417 | 11,370,496 |
| Panel | 0.243135250 | 12,582,912 |
| Node | 0.083822750 | 10,108,928 |
| prefix | 0.046097334 | 10,076,160 |
| restart | 0.083968583 | 10,125,312 |

9 leaves、unsigned frame合計27,288 bytes、署名41,643 bytes、署名込み68,931 bytes。Node/restartのcursor9と累積計上が一致した。native wrapper/transaction/再送費用は含まない。roleタイマーとCPU/RSSは測定範囲が異なり、trusted初期化・artifact index準備等も一部タイマー外にある。Miner追加負担率、実LLM性能、p95/p99や本番capsへ換算しない。

別プロセスMinerはfixture（`a2_miner_adapter_connected=false`）、実A2接続は別のpipeline。Panelはlocal port、prefix/restartはtest frame再検査で、native networkやatomic native journal復旧ではない。

## 再現と証拠

既存shared cacheを使い、互換native RocksDB archiveをpath/hash/envで固定した。debug0/incremental0/coreopt0/jobs2、`RUST_MIN_STACK=16777216`。default stackや本番最小hardwareの認定ではない。新しい巨大target/C++ buildを作成していない。

再現は固定candidateをcheckoutし、既存cacheと互換archiveを `CARGO_TARGET_DIR` / `ROCKSDB_LIB_DIR` へ指定、`ROCKSDB_STATIC`をunsetした上で `python3 testing/positive-court-a7/run.py --out /absolute/fresh/directory` を実行する。実使用path・compiler・feature・stack・argvはmanifestに保存した。

[正式manifest](../../testing/positive-court-a7/evidence/a0-79c3-fixture-fixed-2026-11-01/run.json)、[process測定](../../testing/positive-court-a7/evidence/a0-79c3-fixture-fixed-2026-11-01/process-report.json)、[最終整合確認](../../testing/positive-court-a7/evidence/a0-79c3-fixture-fixed-2026-11-01/integrity-verification.json)。親と別担当がsource集合5,169件、両lock、native archive、両A7 binary、実行42 binary、全12ログ、4 Borsh、process-reportを照合した。12 groupと9 roleの残留なし。raw・旧失敗・digestは加工せず保存する。

未検証: actual176 writer/new job授権/native Env、現native registry/draw/funds/lockに基づく全credit read、実provider/cold/network、完全native journal・DB reopen/reorg/pruning/IBD、実LLM・同時負荷・quantile/承認済caps。A6後続realSourceExternalState等の別phaseは対象外。公開activation・model tierは変更なし、認定したproduction profileは0。A0の後続候補では、その新しい同一sourceで再実行する。
