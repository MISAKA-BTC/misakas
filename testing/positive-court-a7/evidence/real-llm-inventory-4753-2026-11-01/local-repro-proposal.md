# A7 実LLM local repro harness案

2026-11-01。仕様調査の固定sourceは `4753ee4beae5e245095954c6ef6e9b96fa05f361`。本書は既存3モデルを実行するための接続案であり、harness実装・実行・新native V2登録の成功記録ではない。今回の範囲はread-only inventoryと本案まで。Cargo、download、変換、Root/A1/A3のtarget/cache使用は行っていない。

## 入力と固定条件

入力は[集約結果](README.md)と3つのinventory JSONに記録した実checkpoint・tokenizer・declared artifactをそのまま使う。SmolLM2-360M/135Mは元context8192、Pythia-70Mは元context2048。全weight/layerを保持する。失敗時にモデルを選び直さず、toyや縮小modelへ置換しない。短いpromptの実行は短いjobの結果として記録し、context宣言の維持と最大context全位置での実行を別項目にする。

実装を開始する段階でRootが指定する新しい固定candidate、使用可能な独立target/cache、feature、compiler、lockfile、native archiveと実行binary hashを記録する。4753向けのAPI調査を後続sourceへ無検証で適用しない。生成物・job入力・環境変数・全command・stdout/stderr・終了状態も保存する。過去private登録のbinary/source対応を今回のhash照合だけから認定しない。

既存artifactのmetadataは上書きしない。`parse_recorded_metadata_v16`はmarkerなしで旧parserを維持し、明示markerありでcorrectly-rounded-v16を使う。新profileを必要とする場合はRootの別作業として新しい派生artifactを別path/hashで作成し、旧artifactとのprogram/layout/tokenizer/class identity差を照合する。外部に固定したexpected profileがv16である新派生artifactでは、marker欠落・downgradeをharness側で拒否する。歴史unmarked artifactのlegacy成功は対照例として維持し、不明・不正markerは拒否する。Pythiaのplain/declared provenance scale表記差は保存し、packのscale_bits `3e70000000000000`（2^-24）と分けて扱う。

## 明示APIへの接続

| 段階 | Pythia wide | Smol 2モデルのnarrow |
|---|---|---|
| profile | full class/layoutを渡して `ResolvedTirJobProfileV2::new`でprofileを導出し、独立に認証されたclass/artifact/recordと照合 | 既存class・scheme・wireのbyteを保持 |
| row view | `ResolvedTirRowProfileV2::resolve`でrooted schema1 rowとprogramを照合 | I8/I16/I32/Idxの既存4B規約を維持 |
| CPU | `TirClassRunnerWideQ24V2::quote` → `run`/`run_observed` | `TirClassRunnerV1`の既存経路 |
| capture | `TirWideCaptureV2::quote_from_run` → `from_run` → `quote_encode`/`encode_bounded` | `TirCaptureV1` |
| reader | `decode_bounded` → 独立expected bindingの検証 → `quote_auth_expected`/`authenticate_expected` | 既存V1 reader |

実装箇所のpath/hashは[parent-audit.json](parent-audit.json)の `proposal_source_api_files` に固定した。4753の既存汎用SDK/旧backendにwide runnerの自動dispatchを仮定しない。privateな明示APIを直接呼ぶ専用harnessを追加する案とし、存在しない `cpu-run` / `capture2` CLIを実行済みcommandとして記録しない。

full profile resolverはtile数やcontext bound等を確認するが、full layout/admission validatorではなく、class ID再計算は外部認証を代替しない。実geometryの検査は `PalwTirStepSpaceV2::new` / `verify_tir_binding_v2`の担当とする。row viewの `row_max_context` はrooted row由来であり、full layout geometryやadmissionを代替しない。inventory exportから作ったDTOをnative rooted schema1 rowと扱わない。これらの構造検証にも、独立expectedの由来・構築費用が必要になる。

Pythiaはi64 Q24、自然対数単位 × 2^24を保持する。canonical commitment rowのwire lane幅はfinal logitsだけ8B、旧state/intermediateは4B。この幅はexecutor内部値やowner backing全体のサイズではなく、内部i128値等のstorageは各quoteに従って別計上する。i32へのcast、clamp、rescale、旧scheme IDの意味の変更をしない。`PALWTRC2`のbounded readerを使用する。captureは同じ一回のCPU runから構築し、roots生成のためにnarrowで再実行しない。observerの値は元dtypeを維持し、変換はchecked conversionにする。

## jobと実行段階

1. inventoryのfull SHAを再照合する。原tokenizerと保存texts/token IDsの対応を固定する。既存3本の短い英語入力を最初の局所jobに使い、prompt length、prefill/decode、stop条件、job seed、位置範囲を明記する。宣言classを短いcontextに作り替えない。
2. まずCPU実行とcapture roundtripを各モデルで行う。既知のPythia i32範囲超過logitを含む入力で、8B整数とQ24単位がencode/decode後も一致することを確認する。過去最大値は参考値とし、新runで実際に観測した値を別記録する。
3. expected class/artifact/tokenizer/anchor/job/prompt/contextはpeer captureから複写せず、harness側の固定入力から導出する。送信captureのDTOをexpectedの根拠にしない。bindingに含むtrace/step/execution root等も由来を記録する。同一producerのresultからbindingを作っただけの試験は `SELF_CONSISTENCY_ONLY` のroundtripとして扱う。独立expectedを報告する `EXPECTED_BINDING_AUTH` 段階では、別に固定した参照実行の入力/source/binary/rootと導出費用を記録し、candidate captureから得たrootをexpectedへ代入しない。これはデータidentityの比較であり、算術判定やnative creditではない。独立参照が未実行ならその欄はNOT_RUNとする。3 artifactのinventoryだけでは各新jobの独立root群を供給できない。短い任意local jobと、resolverのanchor由来canonical full jobを混同しない。
4. original max contextの全位置実行は別runで計画する。8192/8192/2048それぞれについて処理位置をcoverageとして保存し、quoteと実予算を満たせない場合はNOT_RUN/RESOURCE_LIMITで残す。少数の境界位置や短いpromptで全位置PASSとしない。
5. downstream consumer接続後、同一candidateでprivate native V2登録・admission・永続registry読戻し・再起動・複数Nodeを検証する。旧private testnet12ログ、fixture export、class hash、ローカルreplay一致は、この段階やSecurityCertified/creditの成功を代替しない。

## 費用の記録

quoteはchecked arithmeticによる論理上界。RSS、CPU instruction数、native resource lease、料金を意味しない。`TirWideRunLimitsV2`のpositions/leaves/row lanes/work/requested bytesを実run前に固定する。runner quoteのexecutor/retained/scratch/requested bytesとleaf/node/workをそのまま保存する。

captureごとに `WideCaptureLimitsV2` / `WideCaptureAuthLimitsV2` の全上限とquoteを保存する。独立に保持するartifact mmap/params/plan/space/prompt/expected binding/input frame/run/callback/export bufferをowner表で列挙し、当該operationが既に数えた分を除いて `external_retained_bytes` に一度だけ含める。`moved_leaf_bytes` は `capture_bytes` の内数なので加算し直さない。runのhost leaseをdrop前に解放済みとして扱わない。decode quoteとauthentication quoteを分離する。

| 役割／工程 | 論理記録 | 実測を行う将来run |
|---|---|---|
| Miner plain inference baseline | 同じweight/input・selection条件、retained outputs | fresh processのwall/user/sys CPU、peak RSS、read/write bytes、disk成果物 |
| Panel CPU + capture | runner/capture quote、observer収集量、同時live owners | 一回のrecomputeとcaptureの区間計測、frame bytes、cold/warmを別run |
| Node parse/auth | frame/decode/auth上限と実際のrow/leaf数 | 独立processでdecode/auth計測、モデル推論と混在させない |
| Node/Court arithmetic | 接続済みcheckerの範囲・work/material quote | consumer実装後の別検証。認証済みcaptureを算術判定PASSへ読み替えない |

将来の実測ではbinary/OS/hardware/thread数/環境、warmup回数、反復数、raw samplesを固定し、中央値だけで失敗を隠さない。OS cacheを制御していないfresh processを真のcoldと呼ばない。networkはofflineか実送受信かを区別し、serialized bytesから通信費を推定した値と実通信量を分ける。料金やentitlement/Pの結果は別native経路で確認する。旧fixtureのrole測定値を実LLMの費用へ流用しない。

過去Pythiaの初回はstack値未記録でstack overflowとなり、登録再試行には64MiBのtest stackを指定した履歴がある。将来runでもstack値を明示し、旧A7の16MiB結果を十分性の根拠にしない。初回失敗と明示stack成功を別記録として残す。

## 負例と判定の区別

- wide program/captureをV1 reader・旧schemeへ渡して拒否する。narrowの既存byte列とclass IDは維持する。
- i32境界を超える正負値、8B rowのtruncation、長さのoverflow、予算不足を扱う。未認証frameの宣言長で無制限allocateしない。
- wrong class/artifact/tokenizer/job/prompt/context/anchor、未来version、異常marker、改変binding/root/leafを与え、拒否した段階と理由を保存する。
- syntactically boundedだが計算値が偽のevidenceは、該当APIが許す範囲で保持して後段に渡す。parse/authの成功または拒否を、そのまま算術的な正誤判定にしない。
- `TirWideCaptureV2`はempty leavesによるtrace-only auth成功を許す。full dense captureを報告する場合は同一runのobserver由来leaves数が独立expected.step_leaf_countに等しく、全indexとcanonical geometryを満たすことを別途確認する。empty leavesと1leaf省略を負例として分け、trace-only成功をfull coverage成功へ数えない。dense membershipの認証も計算値の正しさを意味しない。
- quote上限ちょうど／1不足、run+capture+frame同時保持、外部owner漏れ・重複を確認する。キャプチャ生成で二度目のmodel runが発生しないことをobserver/run counterで確認する。

## 次段階の完了条件

実装前にRootと接続対象・固定candidate・利用targetを調整する。まず3モデルの局所実行ログ、quote/実測、captureと独立expectedのroundtrip、負例をA7証拠として保存する。未接続consumerは拒否を維持する。新native V2成功の報告には、対応するparams/fence/validator・資金/UTXO・登録tx・admission・registry・Node再起動の実記録が必要。最大context全位置とclaim単位の抽選検証者認証もそれぞれ独立した未完了条件である。モデル品質・階層の審査は実施しない。
