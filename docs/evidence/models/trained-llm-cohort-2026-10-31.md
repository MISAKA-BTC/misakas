# 学習済みLLM 3件の最大文脈長での登録 — 2026-10-31

> A0取り込み注記（2026-11-01）: 以下は元checkoutの私設登録実験の報告であり、A0のPositive Court実装の完了を示さない。元checkoutだけにある2つのRFC-v2参照を文書名へ直した。[原文の完全コピー](trained-llm-cohort-2026-10-31/original-report.md.txt)を保持し、ログ・digest・実測値は変更していない。

**最終結果: 同じ抽出3件すべて、元の最大contextで私設native登録・別node再生まで成功。** 元実験の2件成功・1件失敗と、修正後の再試験を分けて保存した。 `RFC-v2-0002`（元checkoutの文書） / `RFC-v2-0003`（元checkoutの文書）。前回の小型テストモデルの確認とは別の実験である。

## 抽出

学習済みで一般公開されたモデル7件の明示した候補poolから、seed `18137408927736653350` による重複なし抽出で3件を選んだ。[抽出記録・全候補・固定revision](trained-llm-cohort-2026-10-31/selection.json)。GPT-2、DistilGPT-2、Pythia 70M／160M、SmolLM-135M、SmolLM2 135M／360Mの7件が候補で、random/tiny/testing repositoryは含めない。

初期の空きdisk約8 GiBと実行環境32 GiB RAMに合わせた明示的なpoolであり、Hugging Face全体からの無作為抽出やHub全体の成功率の推定ではない。失敗時の引き直しは行わない。元config・学習を記載したモデルカードとそのhashはrevisionを固定して保存した。

| モデル | 元の最大context | 学習の出典 |
| --- | ---: | --- |
| HuggingFaceTB/SmolLM2-360M | 8192 | [保存したモデルカード](trained-llm-cohort-2026-10-31/HuggingFaceTB__SmolLM2-360M/README.md.txt) |
| HuggingFaceTB/SmolLM2-135M | 8192 | [保存したモデルカード](trained-llm-cohort-2026-10-31/HuggingFaceTB__SmolLM2-135M/README.md.txt) |
| EleutherAI/pythia-70m | 2048 | [保存したモデルカード](trained-llm-cohort-2026-10-31/EleutherAI__pythia-70m/README.md.txt) |

## 手順と範囲

各modelのpinned tokenizerで、保存した3つの通常英文を16 tokenずつ数値化し、校正入力にした。layer削減・weight省略・context短縮は行わない。取得前のheader/shape preflight、file identity照合、streaming変換、3実装conformance、元の最大contextでのlayout宣言、private native登録・永続registry・別node再生を順に行う。

全conformance実装は維持し、`--stream` でtensorを必要時に読む。integer artifactが2 GiB未満でも、全tensorをi128へ展開して複数実装分を保持するとRAM負荷が大きくなるため、driverに明示する選択肢を追加した。合意gateや計算結果の比較を省くものではない。[driver](../../../scripts/models/onboard_hf_checkpoint.py)。

SmolLM2 2件は最終packの3実装conformance、元最大contextでの静的admission、pack自身のdeclared Artifactによるprivate carrier登録・永続化・別node再生までPASSした。[元実験の集計（Pythiaは旧表現で失敗）](trained-llm-cohort-2026-10-31/summary.json)。

native testは既存のprivate testnet-12 launch＋IR fenceのharnessで行う。公開networkへの送信や現行公開presetの有効化、claim認証・報酬の達成とは別である。宣言scope全体の静的admissionと短いconformance jobの一致も分け、最大context全位置の再実行・モデル精度保証を意味しない。

## 失敗と修正

起動用scriptを `select.py` と名付けたことでPython標準moduleと衝突した。モデル実験の起動前に `sample_trained_pool.py` へ改名して再開した。保存済みselectionは変更せず、modelの失敗や引き直しには数えない。モデルの各失敗は `run.json` の該当段階・stdout/stderrに残し、成功例へ置換しない。

モデル単位の資格審査・tier変更、暗号学的なモデル計算証明、claim認証の免除は追加しない。

データ・重み・生成Artifact・全log: `/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/`。

### Pythia-70M の出力表現

元runはi32 Q24の範囲で拒否された。固定した同じcheckpoint・同じ通常英文を公式Transformers 4.57.6 / PyTorch 2.15.0、float32 / eager / remote code無効でも実行し、logit約223.645〜1082.212を確認した。Rust float frontendと同じ値域である。未学習checkpointやcontextの問題ではなく、i32 Q24の約±128という出力表現の制限だった。幅が約859あり、共通offsetだけでも旧表現に収まらない。[公式実装の実測](trained-llm-cohort-2026-10-31/EleutherAI__pythia-70m/hf-reference-diagnostic.json)。

「学習済みなら必ず範囲内」という診断文を修正した。旧profileの閾値は緩めず、[新規RFC](../../design/palw/integration/wide-q24-rfc-draft-2026-10-31.md) / [新規ADR](../../design/palw/integration/wide-q24-adr-draft-2026-10-31.md) をユーザーが2026-10-31に「両案を採択し、実装を進める」と採択。自然対数単位×2^24を保つ別のi64 profileを実装中。元失敗runを保持し、同じrevision・context2048で再試験する。公開presetへの有効化は含まない。

### 採択後の同一checkpoint再試験（wide Q24 v2）

同じPythia revision・context2048・同じ校正入力・全重みで再試験。3実装conformanceはPASSし、既定許容差を変更せず公式Transformersの48位置・50304語彙の全logitsとの比較もPASSした。slope約1.0000、correlation約0.99999、top-1 43/48（0.8958）、KL mean約0.01561、最大絶対誤差約2.009。[pack](trained-llm-cohort-2026-10-31/EleutherAI__pythia-70m/wide-q24-v2/pack.json)。原モデルの精度や最大context全位置の実行を保証する数値ではない。

実Artifactによるv2 captureのroundtrip、旧capture readerの拒否、wide cellの全leaf照合、正しい計算・trace整合の無過失判定、1bitの算術改ざんの検出、誤decode tokenの検出、改ざんされたtrace開示の拒否もPASS。短い実jobで最大logit約1080.698（i64 Q24 `18131109454`）を保持した。[実測](trained-llm-cohort-2026-10-31/EleutherAI__pythia-70m/wide-q24-v2/wide-artifact-court-report.json)。公開testnet-12 presetでのadmission拒否も確認し、公開有効化をしていない。

私設native登録・別node再生も同じclass/root/context/Artifact SHA-256でPASSした。未最適化test binaryの初回は、モデル識別logを出す前にtest threadのstack overflowで終了した。同じbinary・fixtureを、既存release検証と同じ `RUST_MIN_STACK=67108864` で実行してPASS（20.22秒）。driverはtest subprocessにこの既定値を設定し、既存の指定は保持して設定を記録する。モデル・context・合意上限は変更していない。失敗run.jsonはFAILEDのまま保持し、最終結果は[再試験記録](trained-llm-cohort-2026-10-31/EleutherAI__pythia-70m/wide-q24-v2/native-retry-retry.json)で確認する。採択済み本文は [RFC-0043](../../rfc/0043-wide-q24-llm-output.md) / [ADR-0211](../../adr/0211-wide-q24-llm-output.md)。従来の公開claim/FP/V4/device backendへwide profileを黙って流さず、未接続入口は拒否する。公開運用への接続・独立レビュー・有効化は未完了。


## 完了時の確認と残る範囲

[最終集計](trained-llm-cohort-2026-10-31/summary-after-wide-q24.json)。SmolLM2-360M / 135Mはいずれも8192、Pythia-70Mは2048。元の重み・revision・layer数・文脈長を維持し、失敗モデルの引き直しはない。Pythiaだけは明示採択された別のwide profileを使用し、旧i32 scheme・ID・wireを読み替えていない。

Pythiaのpackは `verify --rebuild --stream --strict` もPASS（manifest、sidecar、adapter、descriptor、converter、source、frontend、Artifact、declared class、3実装conformance、HF referenceの11項目）。再構築したArtifactは元とfile digestが完全一致した。[結果](trained-llm-cohort-2026-10-31/EleutherAI__pythia-70m/wide-q24-v2/pythia-wide-rebuild-verify.json)。

SDK build/check、lowererの旧Q24・wide出力の2試験、execのcapture/再生/wide IPC・型・samplingの11試験、取得/driverの13試験はPASS。新profileのcapture・cell・算術/trace/decode courtは実Pythia Artifactでも確認済み。これはモデルの資格審査やclaim認証の代替ではなく、公開networkの有効化・SecurityCertified・報酬は達成したと数えない。公開運用のclaim/FP/V4/device入口への接続と独立レビューは残る。

文書リンク検査は新規不整合0（従来の未解決参照43件は維持）、RFC統合版の41原本・15章の保全はPASS。ADR統合版の204原本・完全付録の照合は維持した。採択済み新RFC/ADRに必要なindex/registry/番号台帳の5変更は元digestを変えず別台帳で照合し、原本・パラメーター台帳を例外扱いできないことを3試験で確認。ADRの全体検査は、本作業で変更していない `docs/governance/parameter-register.md` と輸入時digestの不一致1件でFAILしている。既存台帳を戻したりbaselineを一括更新したりして隠していない。


公開preset/IR fenceの回帰も11試験PASS（[実行結果](trained-llm-cohort-2026-10-31/EleutherAI__pythia-70m/wide-q24-v2/wide-public-fence-regression-current.log)）。初回は並行commit `8cac52ec4` による別件のtestnet-12モデル席数fenceのDAA変更で、先にbuildされたlibraryと更新後のpinが混ざり3つのT12行が不一致になった。元snapshot `ec8d8b856` の20 ruleset・60 IDとは全件一致していたことを[別集計](trained-llm-cohort-2026-10-31/EleutherAI__pythia-70m/wide-q24-v2/public-identity-concurrent-migration-audit.json)で確認し、現HEADを再buildして11試験すべてPASSした。本作業では公開presetやgolden pinを変更していない。
