# A7 — 並列独立検証の完了記録（2026-10-31）

対象commit: `bd962391414c051111387b4b953d89d953467655`。同一候補の11コマンドで **243件成功 / 失敗0**。結果は `PASS_PINNED_PRIVATE_R1_CANDIDATE_ONLY`。A7の今回の実装・独立検証を完了した記録であり、Positive Court全体のnative接続・公開有効化の認定ではない。

base契約・production: A0受理済み `6337ea50d` まで。A1の署名込みingress/meter、A2のbound inference・protected journal tail修正、A3互換修正、A5 node bridge、共通native schema/funding/fenceを含む。A7のproductionコードは同commitと同一。A0の後続P-subject/source port等は対象外で、別候補の成功件数を合算しない。

変更責務: `testing/positive-court-a7/` の独立fixture・pipeline・runner・process制御・監査と本報告。反例、結合API、process/measurementの3担当で並列作業し、親担当が統合候補を固定して検査した。共有module・wire・署名domain・activation・モデル階層・登録規則は変更していない。

| 同一候補の試験 | 成功件数 |
|---|---:|
| 実process制御5件・manifest検査3件 | 8 |
| A7独立反例 | 12 |
| 実A2 → A3 → A1 → A5 private pipeline | 6 |
| 共通契約・funding・namespace・native rows・TIR | 29 |
| A5 accounting | 40 |
| A5 accounting-node | 22 |
| A2 Miner・protected journal | 51 |
| A3 Panel・source fixture | 11 |
| TIR実行器・A1/A3・旧Court回帰 | 47 |
| Positive Court有効化境界 | 8 |
| 旧TIR fence回帰 | 9 |
| 合計 | **243** |

既定ignoreのworker 1件は、親試験が9つの実OSプロセスとして明示実行した。ownerの自己申告や過去の110件成功を上表へ足していない。

接続API: 実 `AuthenticatedTirParamsV2::authenticate` → `infer_and_sign_tir_result_v2` → A3 `PanelAdmission` / `TirPanelBuilder` → A1 `check_authenticated_part` / `CheckedTerminal` → A5 `TirCreditView` / `CreditConsumers`。実A2の署名を維持し、4種類のprompt出力を独立scalar oracleと比較した。別params/class/job/promptは署名callbackより前に拒否する。正しく署名された偽出力、最後のframe欠落、保存Passedだけの状態からはprivate creditを得られない。

A5の経済状態は明示したprivate fixture。7用途のconsumerを通し、局所の計算一致だけではGate C、資金、満期、fraud状態、現在branchを置き換えないことを確認した。native readerは全用途で拒否する。

独立反例は、packet外で先に固定したTrusted、全面的に自己整合する別scopeへの差替え、各leaf破損、不完全prefix、悪意あるPanelによる再署名、正しい計算と偽Miner出力の区別を維持した。全checker入力を実署名込みAPIへ移し、credential境界長、署名込み予算・overflow、復旧prefixへの別scope・欠番・重複混入も検査した。Panel証拠拒否からMiner有罪へ進めない。

process制御は、正常終了、残留writer、TERM無視の親孫を含むtimeout、controller SIGTERM、cleanup中の二重SIGINTを実プロセスで確認した。handlerを復元し、group停止・reap・writer不在確認後にだけログを閉じてhashする。別sessionへ意図的に逃げる子とcontrollerへのSIGKILLは保証範囲外。

| warm 4×4 / 3 decodeの一回実測 | elapsed秒 | process peak RSS bytes |
|---|---:|---:|
| baseline | 0.000730833 | 10,223,616 |
| Miner fixture | 0.019688209 | 11,272,192 |
| Panel | 0.248019792 | 12,500,992 |
| Node | 0.087902584 | 10,698,752 |
| prefix | 0.047184875 | 10,649,600 |
| restart | 0.086868417 | 10,764,288 |

9 canonical leaves、unsigned frame 27,288 bytes、署名41,643 bytes、署名込み68,931 bytes。Nodeとrestartのcursor・累積計上が一致した。native wrapper・transaction・再送の費用は含まない。CPU/RSSはprocess生涯、timerにはfixture/codec/署名等が入り、trusted初期化・artifact index準備は外にある。Miner負担率へ換算しない。

この別プロセスのMinerはfixtureであり、`a2_miner_adapter_connected=false`。実A2接続は別のpipeline 6試験で確認した。process測定を実A2の性能・protected-store custody・実LLM性能と扱わない。prefix/restartはtest frameの再検査で、A6のatomic native journalではない。

[既存学習済みartifactの静的監査](../../testing/positive-court-a7/test-compatible-llm-audit.md)では、確認した学習済み4候補は今回のall-node-commit i32 profile条件を満たさず、比較用tinyQwenも同様だった。5artifactのheader/program・参照JSONとchecker sourceのhashは最終候補でも再照合済み。weights再認証・Rust拒否コード実行・推論・throughput測定は行っていない。これはclaim計算profileとの適合性確認で、モデルの審査・tier判定ではない。

未接続・未検証: A2のnative署名済dutyと保護storeへの実接続、A3の実provider/cold搬送、A4のrooted draw/carrier/deadline/実資金受付、A5のnative Gate C・報酬・DAG・決済のproduction read、A6の要求/試行/資金/支払を含むatomic復旧・電源断・reorg・pruning・IBD、実LLM、複数Node・同時負荷・p95/p99・承認済caps。この候補で認定したproduction profileは0。成功stubによるnative権限付与はない。

再現: `python3 testing/positive-court-a7/run.py --out /absolute/fresh/directory`。debug0/incremental0/coreopt0、build jobs 2、test thread stack 16 MiBを記録した試験環境。isolated lockのregistry 337 package/versionはroot lockの807件にすべて含まれる。

証拠: [run manifest](../../testing/positive-court-a7/evidence/parallel-verified-2026-10-31/run.json)、[process測定](../../testing/positive-court-a7/evidence/parallel-verified-2026-10-31/process-report.json)、[最終整合確認](../../testing/positive-court-a7/evidence/parallel-verified-2026-10-31/integrity-verification.json)。記録対象source 4,658件・HEAD・両lock・両A7 binary・全11ログ・Borsh fixtureを再照合し、全tracked treeがclean、全command groupとrole processにlive残留なしを確認した。実行された25本のRust test binary hashも保存した。各必須test targetで非ゼロ成功を要求する。

失敗履歴: [直前のrun](../../testing/positive-court-a7/evidence/parallel-final-2026-10-31/run.json)は旧filterが0件となり `NO_EXECUTED_TESTS` で停止した。容量調整によるcontroller待機もelapsed/timeoutへ未補正で保存した。filter 1行を実module名へ修正し、本報告の新しい11コマンドを最初から実行した。旧raw・digest・失敗結果は不変。

他担当への引継ぎ: A0はA7専用差分と報告・証拠を取り込み、owner production commitsを重複適用しない。native duty・経済reader・atomic復旧の接続後、その新しい同一候補で全suiteと未検証項目を追加検査する。

有効化設定: **変更なし / 無効のまま**。
