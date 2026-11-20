# RFC-0056／0057／0058 — 検証範囲

日付: 2026-11-17（Asia/Tokyo）

対象は今回の文書改訂、入力保全、SWC1の小さな統合contractモデルである。native実装、暗号方式、実LLM、資料可用性、DAG定理、最低Node負荷、公開1 BPSを実行・証明した記録ではない。

## 抽象モデルの位置付け

[check_work_model.py](check_work_model.py)はSHA-256、LP／NR、小さな整数trace、Merkle openingを使って、登録owner、元Original、段階の閉鎖、一回性、反証、成熟、権利消費の境界を検査する。actor認証、乱数release、stable cut、資金、DB snapshot等には明記されたfixtureを使う。

有効なzero-model traceを安価に構成できるnegative demonstrationを含め、計算費用下界が成立したとの結論にはしない。nonceや局所sampleが独立完全再実行を証明するとの結論もしない。

結果は[model-results.json](model-results.json)に現在の3 RFCとscriptのhashを保存する。旧0056の46件、旧0057の49件は過去snapshotの証拠として変更していない。

## 入力保全と文書検査

[check_revision.py](check_revision.py)は、新たな事実snapshotの[source-baseline.json](source-baseline.json)を固定hashで検査する。公式checkのbaselineは更新しない。

入力は添付docsの2,483ファイルと、前回にだけ存在した44ファイルの計2,527ファイル。旧採択本文、履歴・evidence、旧3 revision、モデルtier、既存78項目のパラメーターを保全し、20 RFC／50 ADRを維持する。現在の0056／0057の改訂は旧bytesと区別して許可する。

新しいリンク問題は、結合入力に実在する問題とのmultiset差で検査する。過去から残るリンクを新baselineで解消したことにはしない。

## 既存検査の失敗

添付docsの時点で、以下の三検査はFAILしていた。

| 検査 | 入力時の結果 |
| --- | --- |
| docs/maintenance/check_docs.py | 既知43件に加えnew扱い930件。実在するリンク問題は計973件 |
| docs/rfc-v2/check_integrity.py | 歴史RFC 7本のbytes不一致 |
| docs/adr-v2/check_integrity.py | 歴史source／appendix／baseline等の86件 |

入力時の出力全文とhashは[baseline-checks.json](baseline-checks.json)に保存した。最終runとの比較は[validation-results.json](validation-results.json)で、文書改訂の検査結果と分離して記録する。

<!-- FINAL-RESULTS-BEGIN -->
## 最終結果

| 対象 | 結果 | 意味 |
| --- | --- | --- |
| 統合contractモデル | **12／12 PASS** | 実hash・小整数trace・局所反証・状態境界。native／暗号／LLMの証明ではない |
| 今回の入力保全・文書統合 | **PASS** | 結合入力2,527ファイル、削除0、旧revision35ファイルと旧パラメーターを保全 |
| 新たなローカルリンク問題 | **0件** | 入力に存在する973件は残し、新しい欠損を追加していない |
| 現行統合文書 | **20 RFC／50 ADR** | 新番号は統合後の変更記録として登録 |
| docs公式check | **FAIL、930件→930件** | 既知43件とは別のnew扱い。入力時から増加なし |
| RFC公式integrity | **FAIL、7件→7件** | 失敗出力は入力時と同一 |
| ADR公式integrity | **FAIL、86件→86件** | error listは入力時と同一 |

独立レビューでtoyモデルの4つの不備を発見し、反例を固定して修正した。完全承認ゼロからのmint、Abort後のnoticeによる復活、GC後の元資本sequence再使用、checkpointのregistration参照差替えである。修正版に対する独立した再実行で、いずれも拒否を確認した。これは今回の小さなモデルの修正であり、nativeコードで同じ攻撃が成立したとの報告ではない。

12件は境界シナリオ群の数であり、個々のassertion数や実ネットワーク試験数ではない。正しいzero-modelのshortcut traceが通る例は、費用下界が証明されたという主張を否定する検査である。

全78項目の旧パラメーターJSONも前回0057版と完全一致している。公式baselineをFAIL隠しのため変更していない。これらの結果からPoW成立、資料可用性、DAG安全性、本番activationを認定しない。
<!-- FINAL-RESULTS-END -->

## 実施していないこと

- native Rustの新経路実装・ビルド・回帰試験。
- Studioから同一claimを送るnative E2E、実署名、実資本、DB、クラッシュ復旧、IBD。
- BLS、DKG、実beaconの暗号監査、ネットワークの未知性／偏り耐性の実証。
- cold Panelへの実資料供給、選択的秘匿と虚偽未着の客観的識別。
- 1万／数万モデル登録下のCPU、RAM、ディスク、通信量、court待ち行列、持続1 BPS。
- 任意モデルの計算量下界、非償却性、独立した物理実行の証明。
- DAG common-prefix、分断後収束、活性、取消・成熟・pruningを含む安全性定理。

設計レビューは[REVIEW.md](REVIEW.md)、sourceの確認箇所は[SOURCE_AUDIT.md](SOURCE_AUDIT.md)、必要なnative検証は[実装計画](../../../design/palw/integration/staged-work-implementation-plan-2026-11-17.md)を参照する。
