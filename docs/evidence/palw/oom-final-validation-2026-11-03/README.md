# OOM 対応の診断・検証記録 — 2026-11-03 JST

記録時点: **2026-11-02 15:13 UTC / 2026-11-03 00:13 JST**。運用値はその時点の観測であり、その後の状態を保証しない。本記録の作成では source を凍結し、文書以外を変更せず、Cargo・SSH・配備操作を実行していない。中央実行ログと root が保存した運用証跡を読み取った。

**production に配備されたのは encoder 対応の先行 `65c52b77772fbc7948133f8c07778ed14e4788e7` のみ。統合候補 `5494107fb71b098824317e9281a9bbc35bafff4b` は未配備。65 でも高メモリ・swap 使用が続き、実 heap owner の特定と全 OOM 解決は未達。公開系のモデル実行復旧・claim 認証/決済の完了は確認できておらず、private branch 問題も残る。**

## 対象と変更の境界

| 対象 | この時点の状態 | 言える範囲 |
|---|---|---|
| production 95 / 先行65 | PID `1616922`、invocation `3c3c023c46014ee0be042c9022fdc34a`、実行ファイル `/root/t12-rel/65c52b77772f/bin/kaspad` | encoder の大きな buffer 保持への先行対応。Node 全体の予約・readiness・新 GC 修正を配備したとは扱わない |
| 中央統合候補 | `/Users/wata/.codex/worktrees/oom-integration/MISAKA-testnet`、branch `codex/oom-integration`、HEAD `5494107fb71b098824317e9281a9bbc35bafff4b` | 後述の段階別 build/実試験に合格。production での持続的負荷・reorg・実モデル実行の合格証拠ではない |
| IBM の統合 release | root が別 source/target で準備中 | 浅い65 snapshot からの clone が終了128となり staging を診断中。release artifact 完成・配備成功とは記録しない。live Node 不変 |
| Linux 5.104 の DHAT 診断 | 65 + heap-only timer patch の隔離 build、jobs=1 で進行中 | 小 smoke 2 ケース成功は root 報告。実 Node の DHAT report は未取得。DB copy も未検証 |

95 の実行ファイル SHA-256 は `4cba41363cdbe8c8ce05c8b10d8829965b3d1cb38ee43c5b72d0dac1ed72e0a8`。65 の source 証跡は12,621 files、source proof SHA-256 `7c49ad2f24e1b1860f50dce3f8fd917e6b4f4f67da1b5818a3ab81c92591d309`。旧4128は通常 shutdown が20分停止待ちとなり、root の記録では第3回 SIGINT による application forced exit、SIGKILL は不使用。独立した cold app backup と SHA/inode 証跡を保持し、元の derived UTXO index は退避、65 で再構築した。正常終了の根因修復を示すものではない。

production の identity は最新 canary で以下と一致した。

| 項目 | 値 |
|---|---|
| params | `253d22bbc32fbcf3f23e9333524435478a5a7b87ac9a360de6513dde4cdc5248` |
| schedule | `ec5f56fa48cdeafb3a6fc953c02a2dcd53f0f54025fe7243a87972884a47eb5e` |
| genesis | `a27f8f44fe4d91a5bed940be9dbd6d260ccb95cc00d948b1c08ddb6bd1a5f02542a6cf35c7a4d959ba4863ac1557861671763e5cc22937c697870283a8ca1f23` |

LLM/モデル計算の暗号学的正しさ証明は導入しない。モデル二階層と claim 単位の Court / Gate C / selected-verifier approval を維持する。保持 fence は **DAA 20,000** の既存候補を維持し、撤回された18,850への前倒し、raw signed horizon の padding、再抽選、旧 claim の遡及変更は行わない。最新 public DAA 19,388 は20,000より前であり、稼働観測だけから fence 後の動作確認を主張しない。

## 65 配備後の運用観測

以下は root 保存の `canary-*.json` からの値。単位は byte。`operationalPass` は RPC/identity/public backend/selected-chain index/address projection 等の検査範囲であり、メモリ安定・モデル計算成功・claim credit・全 OOM 解決の判定ではない。

| UTC | virtual DAA | memory.current | swap.current | operationalPass |
|---|---:|---:|---:|---|
| 13:49:13 | 19,379 | 484,143,104 | 0 | false（配備直後の catch-up） |
| 13:54:19 | 19,385 | 5,347,934,208 | 0 | true |
| 14:02:41 | 19,385 | 8,155,029,504 | 0 | true |
| 14:29:53 | 19,387 | 8,395,878,400 | 0 | true |
| 14:51:43 | 19,387 | 9,325,641,728 | 0 | true |
| **15:11:49** | **19,388** | **9,605,332,992** | **404,320,256** | **true** |

最終 canary の PID/invocation/executable は不変、RPC `isSynced=true` / `isUtxoIndexed=true`、public backend の selected head context と index が一致し、address projection lag は0だった。一方 `Pss_Anon=9,225,188 kB`、`LazyFree=583,504 kB`、memory event の `high=1951` / `oom=0` / `oom_kill=0` を記録した。`MemoryHigh=9,663,676,416` / `MemoryMax=10,737,418,240` / `MemorySwapMax=6,442,450,944` は維持されていた。**OOM counter が増えていないことを、余裕がある・増加が解消した証拠とは扱わない。** PSS/LazyFree/cgroup 値は異なる量であり、単純な差引きで live heap を断定しない。

同じ最終観測の `receiptBlocks=0`、`laneLastWorkDaa=19,254`、`exec-priced-attempt=0`。95 自身は producer off / draws0 / produced0 / panel false。過去の attempt が lane window に残ることは、現在の実モデル実行・公開 chain での新たな認証済み work 復旧を意味しない。

## 統合候補で実装・レビューした範囲

以下は中央 candidate の範囲であり、production65 に入った一覧ではない。

| 範囲 | 実装・レビュー結果と限界 |
|---|---|
| Transport | queue 受付時に retained payload と codec peak を予約、process/connection 枠を共有。受付後の encode で追加 admission を要求せず、Rust/protobuf copy を別予約し、queue→codec→HTTP `Bytes` の最終 clone/slice owner まで ticket を移管。未送信 failure は rollback。sent future cancellation は既存 pending を response/timeout まで保持。既存 legacy timeout 後の late same-op 混線は未解決 |
| Process/native/host memory | class 間で単一 budget を共有、pending と materialized を分離。LazyFree の値取得を整理。reviewed classified cold decode / tracked projection / delta の局所資源不足を typed local-resource として伝搬。全 raw consumer を default admission 化せず、既存 String semantic 経路の互換を維持 |
| UTXO response | index read guard 下の streaming count→cap→生成前予約→index map→Rust vector→protobuf へ接続。copy の同時生存を別加算。script input の ticket も `spawn_blocking` / 未受領結果に随伴させ、外 future cancel で先に解放しない。cursor は最後に返した row を指し、削除された cursor の次の live row を skip しない。wRPC の serializer/queue は未接続 |
| Retention/provider/GC | 新受付の義務は元署名 horizon と「受付 DAA + 受付時必要期間」の max。原署名・claim ID・VRF 抽選入力は不変。provider の `>=` 必要 horizon は緩和しない。foreign GC は resident-only fallible consensus read を使い、cold/missing state/DB/import/parent/DAA mismatch は hold。healthy な claim absence は false として後の期限/件数回収を許し、永久 pin にしない |
| Producer readiness | 選択 parent・必要 bootstrap 履歴・state root・import epoch の同一 view を preparation 前/重推論直前に確認。実行後の epoch/instance 等の publish 保護も維持。新規推論 hold 中も既存 claim/Court 資料供給は継続。孤立枝そのものの fork-choice/resync は解決せず、単一 refusal で全 producer を停止する規則も追加しない |
| Notification control | Broadcaster の control 履歴を authoritative registration map + 1 wake に置換。stop/start の世代を分け、旧 await completion が新 listener を purge しない。scope 更新中の event は既存 plan の意味を保つ。data notification は drop/coalesce/bounded 化していない |
| Shutdown diagnostics | start/stop service join が5秒を超えたときだけ未完 service 名を INFO、以後30秒ごとに WARN。既存 join/error/終了順/90秒 blocking grace は不変。65 の停止待ちを直した・原因特定したとは扱わない |

## 中央の actual build / 試験

証跡ディレクトリ: `/tmp/misaka-t12-oom-20261102/`。試験は root が資源枠を管理して実行した。下表は**段階別の実行ログ**であり、最終 HEAD に対する全 suite 一括実行や全 feature の合格表ではない。異なる filter の重複や child test を合計して独立試験数を水増ししない。`running 0 tests` は合格対象として数えない。

| 対象 / filter | 実際の結果 | ログ |
|---|---|---|
| 65 の pinned vendored-tonic 回帰 | 9 passed / 0 failed。Node 全体ではない | `current-regression-result.json` |
| Core memory budget | 7 passed | `integration-core-test-r2.log` |
| retained transport（初期段階） | 4 passed。後続すべての変更に対する全 test 再実行という意味ではない | `integration-transport-test-r2.log` |
| configured20,000 retention / 29 historical claims / signature・ID・draw 不変 | 7 passed（下記の回帰修正後） | `integration-retention-test-r2.log` |
| default raw transition / default recognized carriage + tracked projection | 各1 passed、計2。ordinary zero quote と explicit ticket guard の分離 | `integration-default-*.log` / `integration-default-tests-result.json` |
| UTXO cap/page/reserved-generation の実 DB 回帰 | 親6 passed。child 出力を別の独立6件として加算しない | `integration-utxo-index-test.log` |
| Broadcaster control・scope・shutdown | 9 passed。bounded Persistent listener の drain 待ちと consumer 再開後の正常 join を含む | `integration-notify-test.log` |
| real compiled Consensus/Node の targeted filters | 10 filter、各1/1/11/1/1/1/1/2/5/2 tests、計26 matched tests が成功 | `integration-real-tests-results.json` と参照先ログ |
| final GC / cache rerun | cold-reader1、foreign GC2、foreign retention1、effective obligation1 が成功 | `integration-gc-final-results.json` と参照先ログ |
| 実 Consensus API retention 回帰、HEAD549 | 1 passed。cold/import/parent mismatch から healthy absence へ復帰する経路を実 API 越しに検査 | `integration-gc-api-test.log` |
| generated schema + tonic の large frame | 1 test 内の256 MiB / near320 MiB × plain/gzip の4ケースが成功、全ケース final reserved=0 | `integration-large-frames.log` |
| Node check / test build | earlier check/r2 と actual Consensus/Node `--no-run` build が成功。GC 呼出しの中間 compile failure は bridge 修正後の build/API test で解消 | `integration-node-check*.log` / `integration-node-test-build-gc.log` / `integration-gc-api-test.log` |

26 matched tests の内訳は cold single-flight、nested native pressure の local-resource 分類と corrupt-frame data error、template parent snapshot11、rfc12_c2、foreign retention、foreign GC reread、effective obligation、producer context2、host ledger5、LazyFree sample2。これらは制約を実 source で検査する回帰であり、稼働 fleet の同期/供給/期限内 Court/連続 reorg/最大 context 実行を代替しない。

large-frame test は standalone harness で actual generated protobuf と vendored codec を使用した。256 MiB ケースの accounted simultaneous peak は1,073,939,029 bytes、near320 MiB は1,342,370,389 bytes。これは予約会計の値であり RSS ではない。実測 wall time/OS memory の付録があっても Node 全体の最大メモリ保証にはならない。production の pinned dependency subset に対する65回帰と、standalone harness の依存解決・fixture の範囲を混同しない。

### 検証が捕捉して修正した回帰

初回 retention 試験は **0/7 passed、7 failed**。native `value_requests_v1` が default でも正の値を返し、ordinary transition は reservation0 のため setup の genesis→class/bond 登録出力を拒否した。元ログ `integration-retention-test.log` を残す。修正 `a5764136b`（中央 `81010b527`）で legacy raw quote を0に戻し、already-tracked owner 用の actual quote を分離。source の独立レビュー後、上記 retention7/default2 が actual Cargo で成功した。問題を署名 padding や consensus rule 緩和で回避していない。

GC の中間 check は ConsensusSession に追加 API が直接見えない `E0599` で失敗した（`integration-node-check-gc.log`）。中央 `21d3ab38a` の checked session API 呼出し修正後、Node/Consensus test build と HEAD549 の actual API test が成功した。失敗ログを「最終成功ログ」として扱わず、後続成功の対象を明記する。

## モデル実行・private branch と identity 診断

B0 の保存観測（2026-11-02 12:09 UTC）は PID136821 / source30acb77e23f8 / P2P `6a349e2d-5fb0-4606-92bc-36579ac66f9b`、DAA19,315、`isSynced=true`、draws56 / produced56 / receipts0。近い時刻の公開側はDAA19,364。params/schedule/genesis は同じでも、B0 は既知 public head を `DominanceViolation` で拒否し、自枝で Qwen 生成を続けていた。`COURT_RETENTION_BELOW_GUARANTEE raw20607 < required20608` も記録された。保存ログ SHA-256 は `dfb954a1791f6405a0d48145d9f3be44b053d401fbe5b10ae4fa3b214c8c6fa5`。この B0 観測は latest と偽らず、private branch / model fleet の復旧完了証跡がまだない事項として残す。既存 claim・key・retention 資料は保全対象で、fork-choice bypass、署名変更、旧 claim の書換えを行わない。

113 の recovery summary 不一致は source false mismatch ではなかった。primary `bcf08f15…`（RPC26314）は full params253/scheduleec5、別 seat7 `944ecf45-6198-4f0d-8141-af1756851db6`（RPC26354）は params `edf472294c89dab0e64e625dbdfcd039b91081b05f89eacf8ba2aa25baa3a189` / schedule `4024a6e74d7aeb170d99e67c6d8fbe7884d71a716ccb778cd1ee526d01781b04`。root が95の peer `169.58.232.113:40092` を seat7 ID と照合した。同 genesis でも full params が違うため summary は正常に拒否される。future-fence 互換 handshake が接続を許すことと両立する。生成・wire bytes fields・比較は source69e3 と中央で同じで、比較 guard の緩和や peer IP 単位の blacklist を追加していない。

## DHAT：実 Node の結果待ち

**実 Node の report は未取得。heap owner は未特定。以下を解析結果として埋めてはならない。**

| 項目 | 15:13 UTC の状態 / 待ち欄 |
|---|---|
| 隔離対象 | Linux5.104.81.23、`/root/misaka-oom-profile-20261102/source65`。65 + heap feature 限定 timer main patch、その他12,620 tracked files は exact source と root が検証 |
| patch | SHA-256 `bda770eb302e20310a68c919cae7b3ec3ba3ba6d2720c9e7f64807af32963203`、production 統合対象外 |
| smoke | root による実 Linux2ケース成功（独立 timer / early normal return）。実 Node のメモリ増加原因を示す結果ではない |
| build | 最初の jobs4 は MemAvailable guard により停止、OOM ではない。jobs1 / CPU250% / RAM12 GiB / swap2 GiB / 最低 available6 GiB / 時間2700秒等の枠で再開中 |
| build profile | `heap`、opt0 / debug1 / noLTO / codegen256、default+evm+heap。production release の速度・allocation timing と同一とは扱わない |
| 診断 DB | cold backup の **meta/consensus のみ**、総9,596,640,563 bytes の転送が約7.3 GiB時点。actor/key/config/material・元 UTXO index を含めず、転送完了・hash検証前。profile Node 未起動 |
| 3分 report の PID / 時刻 / SHA | **待ち** |
| `REPORT_COMPLETE` / JSON検証・fsync | **待ち** |
| end-live (`eb`) / global-peak (`gb`) の上位 stack | **待ち** |
| 同時刻 PSS/cgroup/Swap と DHAT current/peak の差 | **待ち** |
| owner 仮説 / source 対応 / 再現・修正検証 | **待ち** |

DHAT0.3.3 の公開 `Profiler` は Send で、drop 時だけ stack 別 full profile を書く。`HeapStats::get` は aggregate のみ。timer は OS thread で180秒後に singleton を一度 drop し、partial JSON の streaming 検証・fsync・rename 後に完了を記録する。通常の service join を待たず採取し、**診断専用 process exit を行うため DB は破棄対象で production に昇格しない**。3分は収集停止開始の時刻で、symbolication/write 時間は別。allocator mutex 自体の deadlock・OOM/SIGKILL・disk failure 等に対する成功保証はない。

heap feature は通常の MiMalloc を DHAT/System に切り替える。Rust GlobalAlloc の provenance を観測し、C++ RocksDB malloc/new、mmap/model/file-backed/GPU/pinned allocations、thread stacks、allocator fragmentation/free pages、DHAT自身の ignored bookkeeping 等は十分には見えない。DHAT の cumulative allocation (`tb`) を live RAM と読み替えず、異なる時刻の site peak (`mb`) を同時ピークとして合算しない。MiMalloc の LazyFree 再現や production と同じ成長勾配を保証せず、report に目立つ owner がなくても OOM 解消とは判定しない。

## 残る作業と明示的な非達成

1. 実 DHAT report と同時刻 OS/cgroup データを取得し、Rust heap と native/mmap/allocator 残留を切り分ける。現 encoder 対応だけで原因全体が解決したとは言えない。
2. runtime inbound の schema-aware preflight と、decode/route/flow accumulator の最終 owner への reservation 接続は未完。生成した scanner と fixture の合格だけでは inbound OOM を抑えない。
3. critical UTXO notification に durable diff journal / ack cursor がなく、単純 bounded `try_send(drop)` は不可。停止時の一般 cancel・listener 早期 close も未導入。index は tip のみ比較するため、途中 diff 欠落後に後続 diff が tip を進めると、再起動だけでは欠落を検出できない場合がある。
4. wRPC serialization/queue、他の大きな RPC（getBlocks 等）、残 snapshot/native construction の end-to-end admission と所有は未完。今回の限定経路を全 RPC に拡張して主張しない。
5. combined release の isolated build・artifact/source 一致・配備 review・実 fleet 負荷検証は未完。combined はまだ production で動いていない。
6. private branch の安全な復旧、canonical 同一 view の確認、実モデル生成→receipt→claim approval/Court/credit の完了は未達。`isSynced=true` 単独、local matching replay、fixture 署名の成功は、この運用達成を代替しない。

## 証跡の所在

原本は root の incident directory とローカル `/tmp/misaka-t12-oom-20261102/` にある。本節の hash は本記録作成時にローカル原本を読み取った値。raw log の保存責任と source/実行対象の対応を保持し、後から異なる試験や更新された `current-state.json` に差し替えない。`current-state.json` 内の旧 snapshot 項目より、ここで明記した最新 canary・actual result を優先した。

既存の設計/試験資料: [保持・provider](../oom-retention-2026-11-02/README.md)、[producer readiness](../oom-producer-readiness-2026-11-02/README.md)、[notification control と shutdown/index recovery](../oom-notification-control-2026-11-02/README.md)。統合側の補足は `docs/evidence/oom-memory-budget-2026-11-02.md` と `testing/transport-memory/REMAINING.md`。DHAT patch/手順はローカル `/tmp/misaka-t12-oom-20261102/dhat-profile-65/` に隔離保存した。

| 原本ファイル（上記 local incident directory 相対） | bytes | SHA-256 |
|---|---:|---|
| `canary-20261102T134913000473Z.json` | 11114 | `cd7c63e4ff8868f34cf19a85b95d406318ea4271fe5ea77147473eda06f0debb` |
| `canary-20261102T135419626978Z.json` | 11123 | `c854ded39776c665107565a5fbd8dbd958949d356173bfeb545522539acb86ab` |
| `canary-20261102T140241974774Z.json` | 11125 | `779d6b8b69b55abcd6d9fa0e5df21dcd17e58648afe88a020ee84a1f2a58d245` |
| `canary-20261102T142953944293Z.json` | 11125 | `49b49899082210d9895b0b5eac740c8d17f4e7b71df312656b8719e01a44af87` |
| `canary-20261102T145143030144Z.json` | 11125 | `7e589279ec60734dc14427fa39fa0d799db323b89113000d1213d5181a4c3484` |
| `canary-20261102T151149190688Z.json` | 11148 | `f8c70ab6676331fc07cc4e8bf8f8346b04c467ac51042ba22e2d4b4aed527763` |
| `current-regression-result.json` | 19039 | `5714773b1a229f7f19b7422213073854c88490977fcd70b84baf40d4689a61e2` |
| `integration-core-test-r2.log` | 3911 | `7796d6b78a6a5e7193d9c6dc84680e0d394d67a131b1093275fc0e5b00456418` |
| `integration-transport-test-r2.log` | 1077 | `e2a8c524767c74bde1dfa314b21e4c59a16a0373d6fe498ae0fb9e69669a16a5` |
| `integration-notify-test.log` | 33101 | `73f4b9666b783e1d35be9e467f7dc5871952bbac54c72fb44eb5f4ba68d35b53` |
| `integration-retention-test.log` | 14478 | `dec1ed30e41b8c89066ba95f1bc2c6a7727c0dfb7a80bd78a68f4ac04e1e5568` |
| `integration-retention-test-r2.log` | 1495 | `103b3d1797ff22710a07805cdd3e2a96b8bfa773a7d4324dd977c916d402e1da` |
| `integration-default-tests-result.json` | 544 | `a5e18bcb012777bc7ca6db0bd5502d8f5c673ff1eedbdaa6e9f6fdfeb785f268` |
| `integration-default_raw_transition_keeps_zero_quotes_but_explicit_owner_requires_coverage.log` | 429 | `a2572954b872cac34ab297b9ad26cdc32d8461ba2e36162794a95bea59621fcc` |
| `integration-default_recognized_carriage_and_its_projection_keep_process_tickets_without_strict_capability.log` | 443 | `ba3f47892b99ef9c503bb55ba6e065d76155178b73eaf312214ae3607856640b` |
| `integration-utxo-index-test.log` | 5720 | `a170c7cea1020e83ea1337f004f90f9ff10d4b4e026833bc930fa52f90f353e7` |
| `integration-real-tests-results.json` | 3939 | `9c8dd78b99d13c6ce513632eec9bdf3783d750be2fee76807f5205d803b00077` |
| `integration-gc-final-results.json` | 778 | `db978917a8ec549f7b76932ba45c720431db9da20e5f1da32ffdbd0fce9169a5` |
| `integration-gc-api-test.log` | 1756 | `54a488121c681e4ba0501ef78495b7fb5eb518424824aba56f1ce57c77db3426` |
| `integration-large-frames.log` | 5184 | `dae4163c71ad73772e609c9da964f62f472ecba0fe2720a8c1582c1fef227c93` |
| `integration-node-check-r2.log` | 5466 | `68605ad0b9814950222199b0a475aeeb14cea47d7374223db9982158da5e8602` |
| `integration-node-check-gc.log` | 6467 | `e34d53ec17e1a97c1ccf6c86bce6ccee9278ea137f1c1b41fdab193a6c518495` |
| `integration-node-test-build-gc.log` | 5650 | `7edc8d637a5a4ab74fc5579da6259be93c4c17f4e5abcd414c489ff478b2dc14` |
| `b0-rpc-1209.json` | 7900 | `a6e8ce5416381c542c4d4c855e4c3d8a3a4d002e789bc7c3800c1ca6ffb4b2d4` |
| `113-seat7-rpc-1404.json` | 8588 | `4750b025c21808e0e183d0516ff35ffd8864c7727edac1d7f20f1af14ded1d5b` |
| `95-peers-1404.json` | 3246 | `97a184948935b3c9c094ace475e5b06274b0009988399121f4dd18ef664c1faf` |
| `dhat-profile-65/heap-profile-65c52.patch` | 6560 | `bda770eb302e20310a68c919cae7b3ec3ba3ba6d2720c9e7f64807af32963203` |
