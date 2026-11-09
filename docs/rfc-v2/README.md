# MISAKA RFC v2 — 現行20本

## 2026-11-09追加採択 — 公開資料とモデル未保有の一人検証

独立選出Panelに少なくとも一人の能動的な正直者がいることを前提とし、その人がモデルを事前保有せずに全必要資料を取得・全体検証できる新profileを採択した。Minerの有限な公開供給、owner許可に依存しない認証付きcache・再配布、cold取得からの異議権、Nodeの小さなcourtを接続する。最低完全承認数はlogicalに **k_min=1** とするが、一件の署名直後にFinalへ進めず、全検査・異議機会、Qの解消、依存・資金・原期限を[20](0020-verification-gated-final-and-adversarial-models.md)の単一gateで確認する。

[RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md) / [ADR-0218](../adr/0218-public-materials-and-one-honest-panel.md) / [公開裁定v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md) / [今回の改訂](../maintenance/revisions/2026-11-09-public-panel/README.md) / [今回の検査](../maintenance/revisions/2026-11-09-public-panel/VALIDATION.md)。

正常claimの条件付き完了には、正しいMinerの供給・応答、唯一正直なPanelの完全検証、最大合法な虚偽caseの有限終結と実サービス・包含が必要である。承認数1をPanel人数1や性能実測へ読み替えない。旧75／41、旧署名、既存経済値、EXEC 1 BPS、現行20 RFC／50 ADR、過去のレビューを保持する。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。以下の先行採択は当時の範囲で読み、今回の実装・測定・有効化の証拠にはしない。

## 2026-11-09の先行採択 — PALW実行整合性と一人の反証

新claim/profileでは、一人の適格異議をnative未解決状態へ反映し、一人の有効な反証で多数の肯定承認に関係なく元claimを無効化する。必要な検証が未成立なら成功Final・仕事報酬・creditを与えない。Init・Step・Endの全関係、元claimへの認証、同blockの処理順序、保護された救済容量、資料不足、失効と依存を[現行RFC20](0020-verification-gated-final-and-adversarial-models.md)へ接続した。

[RFC-0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md) / [ADR-0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md) / [公開裁定v0.3](../design/palw/consensus/public-adjudication-policy-v0.3.md) / [今回の改訂](../maintenance/revisions/2026-11-09-palw-execution-integrity/README.md) / [今回の検査](../maintenance/revisions/2026-11-09-palw-execution-integrity/VALIDATION.md)。

通常編集先はRFC20冊・統合ADR50冊のまま。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。条件付きの実行整合性を扱い、Miner/Panel本人のGPU使用、全員共謀・永久検閲下の無条件正当性を証明しない。以下の過去日付の採択・検査はその時点の記録であり、今回の保証・実装状態に読み替えない。今回の入力は `misaka_docs_rfc20_exec_1bps_20261108(2).zip` である。

## 追加採択 — EXECの1 BPSとcourt容量

**現行RFCは引き続き20本。** ユーザーの追加指示により、
[07](0007-claims-exec-and-answer-accounting.md)へ期限付きExecCredit台帳、
[10](0010-dag-consensus-and-native-settlement.md)へ事前公開schedule・明示ScheduleId・現在slot生成と1 BPS目標を反映した。
生成権の参照と取引履歴への接続先を分離し、通常tipの伸長で割当を変えない。

1 BPSはEXEC_TX全体の通常時目標であり、1,000 ms slotあたり最大一つの正規blockを使う。
credit・予定表・producer・検証／包含容量の条件を付け、1秒決済や1 DAA/秒とは扱わない。
[05](0005-bounded-resources-and-long-context.md)・[18](0018-interactive-negative-court-and-defaults.md)・
[19](0019-model-free-node-and-bounded-evidence.md)・[20](0020-verification-gated-final-and-adversarial-models.md)には、
添付分析のcourt待機・搬送予算の膨張を解消する全分岐予約・共有・並列化・実モデルの受入条件を追加した。

[追加採択RFC-0048](../rfc/0048-exec-credit-schedule-and-one-bps.md) /
[ADR-0216](../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md) /
[今回の変更](../maintenance/revisions/2026-11-08-exec-1bps/README.md) /
[今回の検査](../maintenance/revisions/2026-11-08-exec-1bps/VALIDATION.md)。

改訂日: 2026-11-08 / 入力: docs(2).zip / 新設計: **ADOPTED_DESIGN_NOT_ACTIVATED**。

**普段読む・更新するRFCを0001〜0020の20本に整理する。**
ユーザーが採択した、モデルを持たないNode、モデル非共有・Miner／Panel寡占の脅威、
非対話positive・対話negative・検証先行Finalを16〜20で定義し、01〜15の適用範囲も更新した。
これは設計採択と文書更新であり、新profileの実装・測定・公開有効化の完了ではない。

[旧番号原文・追加採択の対応](MAPPING.md) / [版の境界・反映方法](MIGRATION.md) /
[採択ADR](../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) /
[変更記録](../maintenance/revisions/2026-11-08/README.md) /
[検査結果](../maintenance/revisions/2026-11-08/VALIDATION.md)。

## 先に固定する設計条件

- **Nodeはモデル本体を取得・保持せず、全推論、GPU、モデル実行環境、Panel workerを必須にしない。** court・cold start・IBD・再起動でも同じ条件とする。Nodeは上限内の認証付き局所断片を検査する。
- **モデル共有やモデル保有者内の正直な参加者を仮定しない。** 悪意ある運営者がモデルを隠し、そのモデルのMiner・通常Panel・providerを全て支配していても、そこだけの承認で成功Finalにしない。
- Minerが元traceと結果を署名して固定し、global poolから独立に選ばれた検証者が資料を取得して全再実行する。canonical traceのrootと宣言出力が一致すれば、自分の記録からpositiveを一括作成できる。
- 正常positiveに中間値を順次照合する対話を要求しない。不一致は対話negativeで局所化し、完成済み反証は直接提出も認める。
- 資料不足・検証未完了なら、反証を作れなくても成功Finalと仕事creditを与えない。計算不正、提供義務違反、未検証終了は別の結果である。
- 小さなpositiveと承認署名だけで、全global検証者の共謀まで防げるとは主張しない。global抽選と誠実性の前提、Nodeの局所検査、資料供給義務の保証範囲を分ける。

## 20本の構成

| RFC | 担当する範囲 | 今回の位置づけ |
| --- | --- | --- |
| [01](0001-inference-and-generative-jobs.md) | 推論・生成Job・入出力 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [02](0002-tensor-ir-and-shared-query-data.md) | TIR・Kernel・共有Queryデータ | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [03](0003-model-onboarding-and-artifacts.md) | モデル登録・変換・Artifact | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [04](0004-model-tiers-and-annual-catalog.md) | モデル二階層・年次カタログ | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [05](0005-bounded-resources-and-long-context.md) | 資源上限・長文脈メモリ・計測 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [06](0006-panels-and-probabilistic-verification.md) | Panel参加・抽選・確率的検証・認証Receipt | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [07](0007-claims-exec-and-answer-accounting.md) | Claim・EXEC・仕事量・回答会計 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [08](0008-claim-dependencies-and-liability.md) | Claim依存・Prefix生成元・未解決責任 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [09](0009-remote-client-and-sdk.md) | Remote Client・SDK・非保管型操作 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [10](0010-dag-consensus-and-native-settlement.md) | DAG合意・EXECスケジュール・Native EVM決済 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [11](0011-bond-budgets-fees-and-payments.md) | Bond予算・配分・Fee同意・決済会計 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [12](0012-authenticated-state-storage-and-ibd.md) | 認証State・保存・Pruning・IBD | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [13](0013-permissionless-bootstrap-and-release.md) | 公開Bootstrap・Genesis・有効化とRelease接続 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [14](0014-court-master-contract.md) | Court統合契約 — 受付・対話・裁定・資金・責任終了 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [15](0015-freeze-deferred-rejected-and-governance.md) | 設計凍結・延期・不採用・未採択・変更管理 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [16](0016-miner-execution-commit-and-material-supply.md) | Minerの実行commitとclaim固有の資料供給 | 2026-11-08採択の新profileを定義 |
| [17](0017-noninteractive-positive-verification.md) | 非対話positiveと独立再実行の承認 | 2026-11-08採択の新profileを定義 |
| [18](0018-interactive-negative-court-and-defaults.md) | 対話negative・局所裁定・提供義務違反 | 2026-11-08採択の新profileを定義 |
| [19](0019-model-free-node-and-bounded-evidence.md) | モデルを持たないNodeと有界な証拠 | 2026-11-08採択の新profileを定義 |
| [20](0020-verification-gated-final-and-adversarial-models.md) | 検証先行Finalと秘匿・寡占を前提にした安全条件 | 2026-11-08採択の新profileを定義 |

## 読む順序と責務

先行した検証設計の変更は **20（脅威・成功条件）→19（Nodeの境界）→16（元claim・供給）→17（positive）→18（negative）** の順で読む。
jobと数値実行は01・02、登録と階層は03・04、資源は05、参加は06、仕事会計は07、
依存は08、APIは09、合意接続は10、資金は11、保存とIBDは12、releaseは13、共通courtは14、変更管理は15を参照する。

新profileの成功は、ResultCommitted → VerificationPending → CertificationReady → Finalの順である。
CertificationReadyは事前認証の成立を表し、それだけでは仕事報酬や合意creditを与えない。
不一致・適格な公開異議・資料要求が未解決なら保留し、期限までに肯定条件が成立しなければ未検証終了となる。

Nodeの承認条件と局所検査は19・20、Panelの独立再実行責任は17に置く。
同じ規範を各冊で別々に定義しない。通常Panelとglobal認証者を同じrole名だけで同一視しない。

## 歴史と現行の区別

以前の旧番号RFC48ファイル（0001〜0047、0042は二つ）を元の完全パスで保持し、今回のRFC-0048を加えた49ファイルを索引化する。
ADRの統合50本も保持し、新しい採択記録ADR-0215をpost-consolidation記録として登録する。
旧番号のbody・wire ID・署名domain・fence・digestを一括置換しない。

2026-10-29の41原文→15冊の統合は、[source-map.json](source-map.json)と89収録区間に残る。
現在の49の旧番号原文・採択記録→20冊の編集担当は[adoption-map.json](adoption-map.json)にある。
前者は歴史snapshot、後者は現在の担当表であり、同じ台帳として書き換えない。
旧0044のPositive Court R2全被覆検査、旧Gate CのFinal後認証もその版の意味を保持する。

新しい規則の優先範囲は[20](0020-verification-gated-final-and-adversarial-models.md)と
[PRINCIPLES §13](../PRINCIPLES.md#model-free-adversarial-verification)に限定する。
[公開裁定の方針v0.2](../design/palw/consensus/public-adjudication-policy-v0.2.md)が新profileの方針である。
二階層、既存経済パラメーター、計算validity proofの禁止、media・モデル改善の延期は維持する。

## 検証と有効化

今回の再検査は、20冊の構成、原文と歴史付録の保持、対応表、追加リンクの回帰を対象とする。
runtimeの完成、未測定の性能、稼働ネットワークの安全性を文書検査で保証しない。
既存入力にある欠損リンクと旧snapshotのハッシュ不一致は[検査結果](../maintenance/revisions/2026-11-08/VALIDATION.md)で別に報告する。

新profileの署名形式・native接続・抽選policy・資源割当・cold資料取得・最悪時のnegative・
reorg・決済の検証が揃ってから、既存手順でactivationを判断する。
以後の通常追記は20冊の担当先へ集約し、設計凍結の例外手続きが必要な変更は新規RFC＋ADRと明示採択で扱う。
