# A7 — 独立試験・プロセス別計測

A7用workspace: `codex/a7-positive-court-tests`。対象は RFC-0044 / ADR-0212 の
private `EXACT_STREAM` prototype。添付分担表の旧0042という番号で既存RFCを変更しない。
暗号学的な計算証明・モデル単位の審査は導入しない。有効化設定は変更しない。

## 対象の固定と統合方法

- 共通基点: `a5800d2dd5d334e89f7a1ddb6e956aedcbf4adf5`。
- 依存実装snapshot: `9d76f4403`。元checkoutの未コミット変更から、
  `consensus/core/`、`misaka/palw/tir/core/`、`misaka/palw/tir/exec/` の変更39ファイルだけを複製。
  元checkoutは変更していない。個別SHA-256は [source-snapshot.json](source-snapshot.json)。
- これはA0共通契約の採択・最新統合候補の認定ではない。A0の契約が確定したらAPIを照合し、
  **同一の統合commit**で本試験を再実行する。
- A7変更は独立したintegration test、fixture、runner、この報告だけ。
  snapshot commitを他担当の実装に重ねて取り込まず、A7試験commitを取り込む。
  Cargo.lockのdev依存更新はsnapshot実装に必要な差分として分離する。

## 再現

```sh
python3 testing/positive-court-a7/run.py --out /absolute/path/to/fresh-a7-run
```

runnerは上書きせず、command・exit code・raw log・source/compiler/binary digest・host情報を保存する。
`run.json`のPASSはprivate prototypeに限る。未接続項目を0や成功に置換しない。
初回ビルド時間はrole計測へ算入しない。role時間はdebug/test profileでの小例実測であり、
LLMのthroughput、承認済みresource cap、mainnet適性を示さない。
runnerは容量を抑えるためdebuginfo/incrementalを無効化し、consensus-coreをopt-level=0で実行する。
これらの設定もrun.jsonへ記録する。実行対象0件のfilterをPASSに数えない。

## 独立に求める期待値

`misaka/palw/tir/exec/tests/a7_support/mod.rs`に4×4のsigned embeddingとscaleを固定した。
別のscalar計算で各logits値・3 decodeのtoken列を求める。TIR実行器・reference evaluator・
Node checkerを期待値の計算に使わない。i32、i64 Q24、負値、同点時の先頭選択、4入力を照合する。
例: 入力1、通常scale `[3,-2,5,1]` の出力は `[2,0,0]`。

## 試験と期待結果

| 試験 | 必須結果 |
|---|---|
| 全logitsを別計算と照合 | 全lane・全decodeが一致し、全coverage後のみPassed |
| 各leafを一つずつ改ざん | Panel witness拒否、MinerのRefutedにしない |
| 全proper prefix、順序飛ばし・重複 | 未完了/拒否、信用なし |
| 誤weightで自己整合するtraceを作り全員署名 | 登録materialとの不一致をNodeが拒否 |
| network/branch/ruleset/input/tokenizer/class/artifact差替え | 正しく再署名してもadmission拒否 |
| 署名domain・鍵の役割差替え | Authentication拒否 |
| yardstick contextの未設定tokenizer | 推論前に拒否し、登録tokenizer固定後のみ実行 |
| 偽のMiner outputと正しいPanel evidence | 全範囲検査・Finish後のみRefuted |
| 未完了・Passed・Refutedと全7信用consumer | prototypeではいずれもnative信用を付与しない |
| journal prefix・切断・ID/資料改ざん・余剰bytes・期限 | 再検査し、不正なPass復元や期限延長をしない |
| 128の重複rangeと独立集合oracle | union/round予算が一致、拒否時に要求枠不変 |
| 不正・重複応答、close再実行 | 累積搬送費を計上、料金はchunkごと一回、資金保存 |
| witness/trace/projection/checkpoint要求 | 軽量Miner契約で拒否、義務を追加しない |

## 実プロセスと計測範囲

integration testが同一test binaryからbaseline / Miner / Panel / Node / prefix検査 / restart検査を
**別OSプロセス**で起動する。Node側working directoryにはpacketとjournalのみを渡す。
Node分岐はモデル構築・executor・params providerを呼ばず、認証付きoperandを含む証拠を検査する。
これは役割別API入力とプロセス分離の試験であり、OS隔離sandboxは設けていない。
さらに偽のMiner出力をRefutedとするNodeプロセスと、改ざんPanel evidenceを拒否して
MinerをRefutedにしないNodeプロセスをそれぞれ実行する。

`process-report.json`は各roleのPID、elapsed、CPU、peak RSS、evidence bytes、検査cursorを記録する。
全体時間にはfixture準備・codec・署名などが含まれる。Minerはartifact index準備も含む。
baselineのinference-only区間とは測定scopeが違うので、差をMiner追加負担率と表示しない。
RSSはプロセス生涯の最大値で、三者の同時peakやGPU/VRAM、保持disk、network実通信量ではない。
再起動は正常なプロセス終了後にjournalを別プロセスへ渡す。電源断・fsync・原子commitは未検証。

## 他担当への接続事項・未検証

確認事項: snapshotの `misaka/palw/tir/exec/examples/positive-court-llm.rs` は
`palw_tir_canonical_context_v1` の後にpromptだけを設定し、tokenizerを設定していない。
このhelperのtokenizerはゼロなので、非ゼロtokenizerのclassはrole admissionで拒否される。
`yardstick_context_requires_the_authenticated_class_tokenizer_before_inference` で拒否と正しい設定後の
実行を再現する。A7 fixtureは登録classのtokenizerを固定した。共有の計測exampleは担当外のため
変更せず、実LLM計測を行う担当へ引き継ぐ。

| 担当 | 必要な統合/計測 |
|---|---|
| A0 | 共通契約版とAPIを固定し、統合commitで同じsuiteを実行 |
| A1/A3 | CoveragePlan/証拠APIの確定後にfixture adapterを接続。演算を増やした独立oracle |
| A2/A4 | provider委任、cold資料取得、Panel応答隠匿、独立carrierでの実Miner対話 |
| A5 | native Gate C、報酬、DAG重み、決済、prefix/依存読取の実経路を検証 |
| A6 | 要求bytes・試行回数・hold/paymentを含むatomic journal、実crash/reorg/pruning/IBD |
| A7継続 | 実LLM、複数Node、同時Court負荷、拒否/交代時の三者別p95/p99/max |

現状のnative信用barrierは常にfail closedであり、全consumer接続済みという意味ではない。
service ledgerはローカル予約簿でありnative coinの送金・holdを検証したものではない。
mock成功、保存済みPass flag、署名一致を、未接続処理の成功に使わない。

残る仮実装: A7による成功stubはない。依存実装はprivate prototypeのまま。
有効化設定: 変更なし。実LLM認定profile: 0。
