# RFC14 / G14 実装進捗 — 2026-10-20

Source: `7fe073ec2`、専用 worktree `rfc14-g14-completion`。武装・他 branch の統合・配備なし。
[引き継ぎ](codex-handover-court-rfc14-2026-10-20.md)と [RFC-0026](../../../rfc/0026-court-policy-master-contract.md)を正本とする。

## 今回の変更範囲

- P0-2 の拒否 gate: 通常 network の readiness record 検証が形式上成功しても、`OneOutsiderCapacityIncomplete` / `ServiceOpportunityIncomplete` で実経路の武装を拒否。隔離 G14 test genesis の既存経路は維持。外部者経路や帰責そのものの実装完了ではない。
- P1-1 / P1-2 の導出部品: original acceptance から notice boundary 3,120 / hard deadline 10,000 を導出。最短署名済み依存 horizon と後続責任を検査。保持と bond の別 claim を含む最大責任を checked arithmetic で求める。state/fold 未接続、T06/T09 未達。
- [ADR-0192](../../../adr/0192-court-service-opportunity-and-shared-pursuit.md): F02/F03 の選択肢と推奨案。Proposed、未採択。新しい数値・notice 等号・tag 230 は決めていない。

## A/A 詳細仕様化（ユーザー指示後）

2026-10-20、ユーザーが推奨 A/A の詳細仕様化を指示。[詳細契約案](rfc26-service-shared-frontier-contract-2026-10-20.md)に、original acceptance の原子性、義務 identity、供給単位と実供給、inclusion 前提の限界、default の判定順、共有開示、全許容分岐の費用を記載した。提案した同一 DAA の最初の accepting block という供給単位と各境界は未採択。

`palw_court_service_v1`（commit `0336b6cea`）は未接続の予約部品。canonical packet identity の期限順に、既存の割当を一切動かさず first-fit で追加する。過負荷は候補全体を拒否し一部の予約を残さない。bytes/work/IO/control の独立した予算を検査する。予約可能性の十分条件のみであり、全ての可行な schedule を発見する最適 scheduler ではない。profile の全分岐集合の導出、実供給会計、codec と block budget の接続、inclusion 前提、node 試験は未完。G14 完成や T04/T05 通過とは表示しない。

## 静的調査

### TIR 局所化の起点

`kaspad/src/palw_tir_arithmetic_scheduler.rs` の `next_public_from_source_v1` は `next_tree_v1` を呼ぶ。後者は `(height, 0)` から開始し、認証された frontier と独立再実行の差分を下る。公開 source の `StepLeaf { index }` 自体は範囲内の名指しに対応するが、この scheduler の開始点は名指しではない。取得 API の存在と filer の最短経路を区別する。

### court 予算と step 搬送

`release/t12-16000` の `palw_court_capacity_v1.rs` を read-only に確認。`palw_court_op_class_v1` は one-move accusation、opening、close、既存の slot 消費 court move を分類し、`palw_court_op_cost_v1` はその object の serialize bytes と close assembly を計上する。これだけで公開 frame のすべての step answer / carrier bytes が court 予約に計上されるとは言えない。実 block admission・搬送 codec までの照合と試験は未完。`pre` へ別 branch の容量コードは統合していない。

### 測定と既存資料

引き継ぎ元には `docs/measurements/2026-10-19-devnet-payload-carrier.md` と `docs/measurements/README.md` は存在しない。別 branch の資料やユーザー側の進行中測定を、現在の snapshot の実測として転載しない。

## 残作業

P0 の正規処理機会、inclusion 前提、共有 frontier の許容分岐の採択後、予約・default・復旧・Final 保留を fold へ接続する。期限の原子的更新、各手番・終了効果・依存契約実データ・signer/relay・受理前 work 上限・T01–T15 は未完。X3 reorg harness、R-D2-NODE、tag 230 判断待ち、実モデル door 検証も引き継ぐ。G14 完成・有効化可能とは表示しない。

## 検証記録

実装 commit: `62ac3c55f`（専用 branch、未統合）。共有 build lock 経由で次を実行し **29 passed / 0 failed / 0 ignored**。

```text
cargo test --release -p kaspa-consensus-core \
  --test palw_court_policy_v1 --test palw_the_release_did_not_move \
  --test palw_activation_pool_is_t12_only --test g14_close_gates
```

内訳: 新規契約部品 3、release identity 9、activation pool 5、G14 gates 12（`testnet12_identity_did_not_move` を含む）。これは部品と既存 gate の回帰検証であり、T01–T15 の統合受入や負荷測定の通過ではない。
`check_docs.py` は既存の欠落参照 6 件で FAIL。対象は RFC-0024 assets の旧ドラフト名 / 測定 evidence directory、RFC-0025 assets の旧ドラフト名 2 箇所、RFC-0026 assets の `inventory.json` / `cross-rfc-normative-scan.json`。今回の ADR に由来する欠落参照なし。索引を再生成し stale index を解消。baseline 未変更。

### 追加検証

`0336b6cea` の予約部品を加え、上記コマンドに `--test palw_court_service_v1` を追加して release で **32 passed / 0 failed / 0 ignored**。既存 29 件と、新規予約の原子性・入力順不変性・予算/重複/期限の拒否 3 件。T04/T05 の統合試験・測定とは区別する。

文書の欠落参照 6 件は、旧ドラフト名 3 箇所を現行の RFC 本文へ修正し、別 branch の evidence と元監査の欠落 JSON は「未同梱・未検証」と明記した。元の digest・測定結果・manifest は変更していない。修正後 `check_docs.py`: **PASS — no-new-regressions / new_issue_occurrences = 0**。既知の欠落参照 43 件は残るため strict clean ではない。baseline 未変更。

### 原資料保持の訂正

`d08bfb25c` の欠落参照修正のうち、4 つの imported asset 文書は元 package manifest の対象だったため、内容を `7fe073ec2` の bytes に戻した。元資料の再現性を優先し、manifest/digest を更新して整合したように表示しない。現行 RFC-0024 本文の未同梱 evidence 注記は維持する。従って現在の `check_docs.py` は既存 assets 由来の 5 件で FAIL。上の PASS はリンクを変更した中間版の記録で、最終版の結果ではない。baseline 未変更。

## X3 の別 branch 修正

`codex/rfc14-x3-reorg-clock` / `e27771b96`（起点 `05fcf7b5c`、未統合）で、header の未来時刻検査と virtual/template が読む instance-local な試験時計を追加。production の時計・132 秒 cap・fork choice・preset は変更しない。

元の `x3_req_5c` は fork が bind 前で、A だけが重い attempt を持つため B の heartbeat work 追跡が長期化する。さらに pre-Final hold を heartbeat だけで Final branch より優先させる return は既存の経済的 fork choice と両立しない。初回実行は時計拒否を除いても長期化し、中断した。成功とは数えない。

修正では予約と session の rollback を別々のテストにし、fork より後の変更を既存の 2 DAA 内に限定。両枝は同じ DAA / blue work とし、正規の `SortableBlock` の hash tie-break を満たす枝を作る。A/B 各 root、予約、session、phase の rollback と復元を node で確認する。B を Final にしてから hold 付き A が heartbeat だけで勝つという旧シナリオは、達成済みと表示しない。

release `x3_req_5` 対象 **4 passed / 0 failed / 0 ignored**。同じ release binary の `t12_validation_clock_preserves_the_lead_cap_and_retry` **1 passed**。LG14-A 関連 suite は **33 passed / 0 failed / 2 ignored**、この branch の t12 同一性は指定 3 系統を含む **26 passed / 0 failed / 0 ignored**。R-D2-NODE の 2 件と tag 230 の codec 判断は解消していない。

生ログ・SHA-256・コマンドは [release 試験記録](../../../evidence/palw/g14-completion-2026-10-20/README.md)に保存。

## 供給会計の追加と、全共有分岐の量の確認

`0ea164766` で、最初の accepting block / 同一 DAA の追加 block / DAA skip を区別する供給会計部品を実装。他の court 操作が予約 quota を使った block は供給として認定しない。予約のある DAA だけ履歴を持ち、空 block・永久沈黙でも観測 clock は進む。`MissingSupply` から相手の default を認定する API はない。予約後付けや逆行 DAA、予算超過は state を変更せず拒否する。

対象 release **36 passed / 0 failed / 0 ignored**。このうちサービス部品 7 件は予約の原子性、費用の各次元、重複 DAA、供給欠損、部品の rollback/replay を確認。codec の実費用導出、物理 block budget、署名・手番・inclusion 前提、実 fold の期限補正/Final hold、snapshot/IBD は未接続・未検証。P0 完了とはしない。

`4a8d862f6` の release 試験 **2 passed** で、現行 TIR codec の全 canonical 末端 frontier は `N` leaf nodes を分割し、frontier 値だけで `64N` B を運ぶことを確認。`N` は context 長ではなく `step_leaf_count`。synthetic `N=2,000,000`、条件付き `B=12,500 B/DAA` ならこの部分だけで 10,240 DAA。実モデル・実帯域の測定値や一般的な不可能性ではない。[条件と未被覆](rfc26-shared-frontier-volume-2026-10-20.md)を明記した。一経路の door が小さいことを F03 の全分岐容量へ転用しない。

devnet 結果ファイル `docs/measurements/2026-10-19-devnet-payload-carrier.md` は、2026-10-20 の再確認でも release/t12-16000 の作業場所、pre-rfc24、pre-integration の 3 箇所にはない。ユーザー側の測定ジョブの停止・失敗を意味しない。

次は、actual public profile から owner に依存しない packet identity と閉じた合法要求集合を導出し、実 codec の費用・versioned journal・block admission・Final/担保/証拠の保持へ接続する。F03 の範囲と全分岐の byte/work/IO/control/保持が閉じるまで武装拒否を維持する。P1-1〜P1-5、P2、T01〜T15 の残り、R-D2-NODE、tag 230、実モデル door の完了条件を縮小しない。

## Rooted service ledger と native default の拒否検査

`palw_court_service_state_v1` の候補は、global な予約/供給 book と immutable original scope / `+10,000` / packet 集合の binding を一つの `staged_courts` row に保持する。packet の claim 間使い回しと残余 quota の不整合を拒否する。key 11 / row 17 は内部 enum の末尾追加で、旧 tag・lifecycle tag 230 の意味は変更しない。native heap 会計と allocation-free wire scanner を追加した。採択済みの codec/fence として本番に有効化したものではない。

native TIR default の pure transition が返った後、row と経済効果の書き込み前に、この book に binding のある claim を検査する。欠損・未完了では default を拒否し、全 quota が供給済みでも未採択の duty eligibility / inclusion 前提を満たしたとは扱わず拒否する。binding のない旧 claim は変更しない。

release の `rooted_service_guard_denies_default_and_native_undo_restores_legacy` は **1 passed / 0 failed / 0 ignored**。同じ沈黙から旧経路では native default が実行される一方、service binding のある経路では供給欠損・供給済みの双方で default を拒否し、状態を一切変えないことを確認。row の Borsh 復元と、実 native `StagedCourtSeed`（delta 200）の undo / forward replay による root 一致も確認した。carrier の署名は fixture の verified-half を使っており、実署名の ingress/node 検証ではない。

原子的 original admission と実 accepting-block のコストからこの row を作成・更新する production writer は未実装。現在の owner demand と owner 非依存義務の対応、専用 fence/profile、Final/担保/証拠/pruning の保留・復旧、全 claim の容量保証、full snapshot / restart / pruned IBD は残る。default 拒否の試験を P0 完了へ読み替えない。別 branch `4f41081ee` の搬送会計も未統合で、その bytes/work/memory と service の bytes/work/IO/control は単純な読み替えができない。

追加の release 回帰試験は **39 passed / 0 failed / 0 ignored**（候補 row の codec/native heap/wire preflight、binding/残余 quota の改変拒否 3 件、既存部品・G14 gates・t12/release 同一性 36 件）。上の native journal 試験 1 件と合わせて 40 件。`cargo check --release -p kaspa-consensus` も成功。生ログと SHA-256 は既存の [release 試験記録](../../../evidence/palw/g14-completion-2026-10-20/README.md)に追加した。文書検査は imported assets の同じ欠落参照 5 件が残り、baseline を変更していない。

## 未解決 service binding の Final・担保・証拠保持

前項の default 拒否だけでは、別の expiry 経路から Final/retirement/証拠回収が進む余地があった。候補 binding のある claim に同じ rooted hold predicate を接続し、DL-1 と deadline 整合性検査、自動 sweep、native Final/retirement の入口、pending accusation、custodian の exit hold、panel liability/Valid lock の時刻 pruning、TIR 証拠 row の TTL と直接削除を保持した。元の 3,120 / 10,000、signed frame の期限、既存 wire/preset の値は変えず、最大 DAA を新 deadline に使わない。book を更新しても他 claim の deadline を書き換えない。

book の writer と snapshot の整合性検査は、binding が元 frame/claim を失った場合や original scope を替えた場合、unresolved のまま Final になった場合を拒否する。保持 row は TTL sweep の 64 件枠を取る前に除外する。外部 provider の署名済み horizon を延長する規則ではなく、既に state にある証拠と未解決責任を保持する候補である。actual original admission の契約 horizon 検査はまだ必要。

レビューで、直接削除の拒否が staged witness の完成時にも作用し、正当な fraud 裁定を止める点を発見して修正した。保持中は既存 partial prefix を残して assembled evidence を裁定へ渡す。V1 の completed witness row 禁止を変更しない。旧 claim では従来どおり witness を消費する。prefix の保持を完成 evidence 全体の archive として表示しない。

最終版の release `--lib rooted_service_` は **3 passed / 0 failed / 0 ignored**。default 拒否、hard/TTL 後の Final/retirement/削除拒否、qualified custody signer だけの exit hold、元の期限値の保存、native journal undo/replay、hard 後の受理時点を含む full carriage の `into_state` 復元、保持中の streamed witness による CourtFraud を確認した。carrier/original は既存 fixture の verified-half で、ネットワーク署名検証や node の IBD ではない。

正常応答・正当な裁定ごとの解除、残余容量内の再割当、hard 後の明示的 recovery、actual admission/block supply、全 owner 共謀下の共有経路、依存契約 horizon、物理容量と保持費用、node の restart/pruned IBD は依然として残る。binding が永久に unresolved で止まるだけの候補を G14 完成とはしない。武装・統合・配備なし。

最終コードの回帰 39 件と同じ release binary の旧 witness 消費 1 件も成功し、今回の対象は native 3 + 回帰 39 + 旧 witness 1 = **43 passed / 0 failed / 0 ignored**。t12 identity / release pin を変更していない。最新ソースの `cargo check --release -p kaspa-consensus` 成功。生ログ・コマンド・SHA-256 を [release 試験記録](../../../evidence/palw/g14-completion-2026-10-20/README.md)へ追加。T01–T15 や G14 全体の完了数ではなく、保持候補の限定した回帰結果である。

## 供給不足の再割当と legacy clock の不成立確認

`repair_missing` は過ぎた未供給 slot だけを元の適格区間内の将来残余へ移し、packet ID/費用/区間/hard、供給済み履歴、他の予約を変えない。同一 claim の全不足分が入らなければ一部の仮割当も残さない。global wrapper は original hard/acceptance/claim ID の順の候補で、回復失敗 claim は保持したまま、残余で可能な他 claim の quota 修復を記録する。決済 prefix の独立進行を意味せず、最適充填や全義務の回復保証ではない。

native helper は book の観測 DAA が native accepting context と一致しなければ書き換えず拒否。成功時だけ既存 delta 200 の一つの global row を更新し、変化がなければ journal も増やさない。binding、Final、担保、証拠の hold と default 拒否は成功時も保持する。actual reply deadline は動かしておらず、原子的な期限補正と適格応答との対応は残る。production caller も未接続。

release 回帰 **45 passed**、最終 native `rooted_service_ --nocapture` **4 passed**、合計の今回対象 **49 passed / 0 failed / 0 ignored**。native ケースは journal の undo/replay、carriage からの復元、stale clock の拒否、保持の継続を検証する fixture である。最新 non-test コードの `cargo check --release -p kaspa-consensus` も成功。生ログと SHA-256 は [試験記録](../../../evidence/palw/g14-completion-2026-10-20/README.md)に保存した。

ただし最初の owner-based native case は **InvalidInterval で失敗**。同じ tiny fixture の導出値は original `a=100`、`preparation_until=220000`、owner 用に選んだ区間 `220003..220013` に対して hard `10100`。hard を広げず、その拒否を回帰条件として残した。journal の成功 fixture は同じ frame の licence 後・準備区間での book 更新であり、owner 経路の成功とはしない。

[準備 clock の起点調査](rfc26-shared-frontier-volume-2026-10-20.md)では、legacy `network_v1` の 4,096/4/64 と式から、quoted preparation の保守的下界だけで **12,420 DAA > 10,000** になることを確認した。これは runtime の測定下界ではない。既存 quote の流用では新 clock を満たせず、次は actual profile/config と logical walk・publication・WorkDebit の対応、および公開 frame 局所化の資源と clock を分けて検証する。既存 ID、signed horizon、再実行数、モデル範囲、上限を無断変更して通さない。P0/P1/P2、T01–T15、R-D2-NODE、tag 230、actual door は未完了のまま維持する。

## X3 の現行 clock と legacy 下界の適用範囲

X3 起点 `05fcf7b5c` の現行実装を確認すると、preparation は whole pass ごと、owner/response は turn ごとの WorkDebit に既にまとめられている。`palw_tir_frame_cost_v1` の de-stacked clock と `kaspad/src/palw_tir_public_custodian.rs` の pass budget は、上記 legacy `network_v1` quote と区別する。12,420 DAA は legacy 式の条件付き下界であり、X3 の現行 profile や全ての許容設計に同じ下界があることを証明しない。既存の元資料・数値を変更していない。

X3 でも全 preset の frame-cost は None、pinned cost record は空で、actual measured network profile と door ≤ 3,120 はまだない。また実 filer は staged witness の accepted offset を読んで次の part を送る。byte throughput と別にこの直列待ちを計上する補助案を ADR-0193（別 branch、Proposed）へ記録している。十分長い collection window が待ちを覆う場合もあり、全既存 plan の期限不足を主張しない。候補 version 2 は旧 version 1 の意味と bytes を保持し、同じ既存 maximum door を越える plan を拒否する。採択・武装・統合・配備や G14 完了ではない。

X3 候補 commit は `0c3cb9f93`。release native plan 2 件 + t12/G14/activation/release/transport/version discrimination 回帰 29 件 = **31 passed / 0 failed / 0 ignored**、consensus release check 成功。旧 fixture の byte-for-byte plan 再導出、collection window が待ちを覆う場合、同一の既存 test-only 設定での collect/material/bond 反映、元 door cap の拒否を確認した。初回の試験想定の失敗と最終版の生ログ/SHA-256 は別 branch の `docs/evidence/palw/filing-clock-v2-2026-10-20/README.md` に保存。X3 の文書検査は新規 0 件/既知 43 件で PASS、本 branch は imported assets 由来の同じ 5 件で FAIL。baseline と元 assets は変更していない。これらのコードを本 branch や pre へ統合していない。

## 依存 horizon の保持期間検査を修正

`CourtClockBounds` は hard + followup のみを検査し、hard + retention が長い場合に短い依存契約を許す不足があった。両方の最大を checked arithmetic で検査するよう修正。境界 −1 / 一致 / +1 を保持優位・followup 優位・同値・ゼロの 4 条件で検証し、別 claim の bond 責任は解放期限だけに残す。release 対象 **29 passed / 0 failed / 0 ignored**（t12 同一性を含む）。生ログと SHA-256 は [試験記録](../../../evidence/palw/g14-completion-2026-10-20/README.md)へ保存。production caller は未接続。

[依存契約の実コード監査](rfc26-dependency-horizon-audit-2026-10-20.md)では、EXEC 報酬用の最大 horizon と session 用の最短 path、main/semantic の別 pledge、provider exact subject/live 状態を区別した。欠落 lookup や単一の最短 horizon 引数を完全な依存集合の保証にしない。実受付での全依存集合の導出・認証、原子的な受付への接続、T09 は未完了。

ユーザー側の結果の再確認では `2026-10-19-devnet-payload-carrier.md` は見つからず、release 作業場所の `2026-10-19-rothschild-mldsa-payload.md` はノード受理・bytes/mass が未測定と記載している。これを actual carrier capacity として使っていない。測定ジョブの状態を推定するものではない。文書検査は imported assets の同じ欠落参照 5 件で FAIL、baseline・元資料の bytes は変更していない。

## Service binding と実 custody の下限接続

`CourtServiceState::check_context` に、main の受理済み custody pledge が original hard を覆う検査を追加した。semantic component のある frame は、実際の受理済み bundle/history pledge の存在、claim/resource/job/producer scope、horizon も必須とする。row writer と carriage/state 復元の既存 context 検査がこの下限を呼ぶ。期限や署名の書き換え、unbound な旧 claim の変更はない。

実コード調査では、現行 original 入口が全 tail 付き job を拒否しており、original.execution_root を EXEC root として扱う経路は作れないことも確認した。これを新しいモデル/claim 除外による達成へ転用せず、実際にその依存を持つ native admission ごとの導出を続ける。詳しくは [依存監査](rfc26-dependency-horizon-audit-2026-10-20.md)。

release native 4 + 回帰 45 = **49 passed / 0 failed / 0 ignored**（t12 identity を含む）。main pledge の境界 −1 / 一致 / +1、既存の Final/担保/証拠 hold、default 拒否、fraud 裁定、native journal/carriage、再割当を確認。non-test consensus release check も成功。native fixture は従来の synthetic verified-half で、署名 ingress・実容量の測定ではない。生ログと SHA-256 は [試験記録](../../../evidence/palw/g14-completion-2026-10-20/README.md)に保存。

semantic 分岐の専用受入試験、profile の後続責任/retention 全体、全 EXEC/provider 依存集合、原子的 original admission、T09 と G14 全体は未完了。この最低 hard 検査で署名済みの配送義務を延長しない。文書検査は同じ imported assets の 5 件で FAIL、baseline・元資料は未変更。武装・統合・配備なし。

## Semantic custody guard の分岐検査

`rooted_service_semantic_custody_guard_rejects_missing_short_and_substituted_contracts` を追加。bundle 欠落、semantic pledge の期限 −1 / 一致 / +1、claim/resource/execution root/job pin/producer の独立した差し替えを検査した。既存 main frame に明示的な guard-only semantic fixture を加えた試験で、native original の署名・課金・credit・snapshot 受付の成功とはしない。production code/params は今回変更なし。

release 対象は新 guard 1 + 既存 native 4、同一性/G14/activation/release 回帰 26 の **31 passed / 0 failed / 0 ignored**。t12 の固定値は変更していない。生ログ・コマンド・SHA-256 は [試験記録](../../../evidence/palw/g14-completion-2026-10-20/README.md)へ保存。文書検査は imported assets の同じ 5 件で FAIL、baseline と元資料は未変更。実 semantic original の受付、全依存集合、T09、P0/P1/P2 と G14 全体の未完条件は維持する。

## Court signer と既署名応答 relay の確認

[実受理経路の監査](rfc26-court-signer-relay-audit-2026-10-20.md)では、literal court signer の bond 鍵での payload 認証と、外側 carrier の UTXO/手数料を区別した。新しい署名方式や鍵委任は追加していない。実 ML-DSA で先に署名した Begin/Part/Finish を、異なる carrier inputs から抽出し、実署名検証を含む native apply に渡す試験を追加。署名鍵を relay 前に drop しても同じ native root へ進み、wrong domain は state 不変で拒否し、Finish で元 leaf body を認証した。

対象は新試験 **1 passed** と、既存 release executable の t12/G14/activation/release 再実行 **26 passed**、計 **27 passed / 0 failed / 0 ignored**。production code は未変更であり、同一性の 26 件は新規 cargo compile とは記録しない。生ログ・コマンド・SHA-256 は [試験記録](../../../evidence/palw/g14-completion-2026-10-20/README.md)。

original/custody の fixture は verified-half、outer inputs は未資金化の codec/native 試験であり、mempool・block の UTXO/手数料・native original の実署名受付・実 PC 停止の T08 達成ではない。新しい適応的応答には authorized court signer が必要で、既署名の relay から signer 常時不要とは結論しない。F02/F03、実容量、全依存集合と G14 の残件は維持する。

## 資金付き legacy relay の node 受理

LG14-A harness に、producer の署名済み応答を newcomer が別の資金 UTXO で運ぶケースを追加した。正直な legacy claim の実生成・bind/license、newcomer の登録、署名済み reserve/demand を経て応答を作成し、その後は newcomer の carrier 署名だけで mempool、node template、block 受理、native fold を通す。元 payload の不変、producer の担保と fixture funding entry の不変、node の回答義務解消、公開 read からの binding 学習を確認した。直接の virtual UTXO 保持照会や実 PC 停止はまだ検証していない。

release 新 node 試験 1、同じ新 binary の既存追及回帰 1、以前の release executables による同一性/G14/activation/release 回帰 26、計 **28 passed / 0 failed / 0 ignored**。production code は未変更。最初の回帰 driver の binary 名誤指定（試験未実行・exit 127）と修正後のログを含め、[試験記録](../../../evidence/palw/g14-completion-2026-10-20/README.md)に生ログ/SHA-256/コマンドを保存した。

これは隔離 LG14-A harness（genesis/test keys、PoW 省略、既存 legacy fence の test-only 設定）での MaterialDisclosedV2 経路。TIR original/TFP1、全許容分岐、signer 不在での新しい適応的応答、T08 全体、F02/F03 や実容量の完成とはしない。[signer/relay の対象と残件](rfc26-court-signer-relay-audit-2026-10-20.md)を維持。文書検査は同じ imported assets の 5 件で FAIL、baseline と元資料は未変更。武装・統合・配備なし。

## Native source からの共有 query 集合

`palw_court_tir_requests_v1` を追加。実 frame/source の全 7 種の query shape を checked cardinality と ordinal に導出し、全 level、leaf、重なりを含む全合法 run、row/tile/IDs を含める。全 query の Vec を事前作成せず、指定 ordinal だけを native validator でも確認する。identity は claim、native allocation instance、native rules、source、shared profile に結び付き、owner/relay/先着順は入力にしない。

最終 release 部品 2 + fresh t12/G14/activation/release 回帰 26 = **28 passed / 0 failed / 0 ignored**、non-test consensus release check 成功。実 tiny source の validator を oracle にした完全集合比較、重複なし、source 差し替え、各 family の境界、最大 Run、profile 分離、端数と overflow を確認した。大きい N の例は synthetic arithmetic で、モデルや帯域の実測ではない。生ログ/SHA-256 は [試験記録](../../../evidence/palw/g14-completion-2026-10-20/README.md)。

[適用範囲](rfc26-shared-tir-request-set-2026-10-20.md)のとおり query identity だけであり、全合法対話 graph、response role/turn/fragment、実 codec/work/IO/memory/保持、原子的 admission、public ingress、実供給と T05 は残る。gate は解除せず、上限・wire・preset は変更していない。文書検査は同じ imported assets の 5 件で FAIL、baseline/元資料は未変更。

## Funded relay UTXO / full-IBD extension

Legacy funded relay now checks actual virtual UTXOs and full selected-block replay into a fresh consensus instance, including equal roots and a fresh verifier read. Extended node test 1 passed; existing release identity executable replay 26 passed. This does not cover process restart, pruned import, TIR-original ingress or service supply. See the evidence README for exact commands, hashes and scope.

## Retained stage timing prerequisite (2026-10-21)

隔離候補 `codex/rfc14-g14-integration-candidate` で、native Begin/Part の受理時刻が異なっても retained owner row/current duty が同じになる反例を確認。割当 DAA の開始時に前提が成立済みかを current metadata だけで判定できない。全 node history が同一という主張ではない。

[ADR-0195](../../../adr/0195-rooted-court-response-eligibility-history.md) / [RFC-0028](../../../rfc/0028-rooted-court-response-eligibility-history.md) を Proposed として追加。新 marker runtime は未実装。最後の段階の遅れだけで免責・責任を決めず、全 causal path、original admission、実 supply、配送前提の未完を維持する。production Rust と既存 wire/clock/cap/t12 固定値は未変更。

重複を除く選択結果は **54 passed / 0 failed / 0 ignored**（fresh core native 28、以前の identity/gate executables 再実行 26）。targeted 1 は 28 に含む。[raw log・コマンド・SHA-256](../../../evidence/palw/g14-integration-candidate-2026-10-20/README.md#retained-stage-timing-counterexample)。文書検査は同じ imported 5 件で FAIL、baseline は未変更。G14 全体の完了・有効化ではない。

## Native prerequisite history runtime candidate (2026-10-21)

`e82921ee1` 後の隔離候補で RFC-0028 の rooted marker を実装。別 key/row 13/19、payload 1 に actual native context を保持し、実段階変更時だけ記録する。後付け binding や WorkDebit/read で時刻を補完せず、旧タグ・期限・t12 固定値を維持。stage/marker は同じ journal に入り、失敗時に共に巻き戻る。未解決 binding に必要な履歴を TTL 後も保持し、binding/profile の矛盾する更新を拒否する。

選択結果は **64 passed / 0 failed / 0 ignored**（fresh native 30、fresh codec/service 8、fresh identity/gate/activation/release 26）。targeted 2 と intermediate 1 は30に含み加算しない。non-test consensus check と再生成一致も成功。[コマンド・raw log・hash・対象範囲](../../../evidence/palw/g14-integration-candidate-2026-10-20/README.md#exclusive-hard-boundary-verification)。文書検査は同じ imported 5件で FAIL、baseline/元資料は未変更。

verified-half fixture と in-memory carriage/journal の検証であり、node ingress/restart/pruned-IBD、全 WorkDebit/restart 分岐、全 causal graph、original 全分岐予約、owner 非依存 public frontier、実 supply、費用・物理校正、依存/終了規則、T01–T15 と G14 全体は未完。最後の stage の eligibility だけで免責・責任を決めず、default guard は閉じたまま。公開 schema/数値の採択・pre 統合・武装・配備なし。

## Shared source-query data primitive (2026-10-21)

`705b27616` 後の隔離候補で、owner/relay/先着順から独立した全 query scope、実 ML-DSA の request intent、重複統合・別 choice の共存、認証済み公開 unit と full-ID seal 後だけの共有を追加。別 source/profile/network/key/context/coordinate の代入を拒否する。全 source-unit の body/parser/hash/RAM 上限成分を query 全列挙なしで導出し、native tiny oracle・ragged tree・overflow で確認した。未知の署名/搬送/IO/保持/全対話費用は未完で、ゼロでも完全な予約証明でもない。

[ADR-0196](../../../adr/0196-owner-independent-shared-tir-query-data.md) / [RFC-0029](../../../rfc/0029-owner-independent-shared-tir-query-data.md) は Proposed。旧 tag245 の digest registry は read-only 監査し、native source/署名/accepting-clock の代替にせず、取り込み・再解釈・武装はしない。新 primitive は in-memory、native の lifecycle/有料受付/応答義務には未接続。

選択結果 **67 passed / 0 failed / 0 ignored**（11 + fresh identity/gate 26 + fresh core native replay 30）。non-test consensus check・既存生成器の一致も成功。[raw log・初期失敗・hash・範囲](../../../evidence/palw/g14-integration-candidate-2026-10-20/README.md#owner-independent-shared-source-data-candidate)。初期 private-field compile 失敗と verifier の message/signature 逆順による9 pass/2 fail は保存し、最終件数へ加算しない。

登録・original は fixture、実 node の全 owner 共謀/埋め直し T05・funding/UTXO・原子的 original 全分岐予約・public ingress・実 supply・新鮮な signer 可用性・依存/終了規則・R-D2-NODE・tag230・実 door <=3,120・T01–T15 と G14 全体は未完。モデル審査、cap/clock 引上げ、数値採択、pre 統合・武装・配備なし。

## Rooted shared-data adapter (2026-10-21)

`b318ea946` 後、RFC-0029 の native data adapter 候補を接続。別 key/row 14/20・15/21・16/22、payload1 を末尾追加し、actual original/class/layout/source/service-derived profile から scope を再構成する。実 requester 署名・既存 floor を検査し、actual accepting block/DAA を保存。raw body/全IDs は write/read/carriage で再認証し、保存 digest や認証済み bool に依存しない。履歴・data・context と既存 journal/indices/保持を接続した。

選択結果 **69 passed / 0 failed / 0 ignored**（native31、query11、codec1、fresh identity26）。targeted1 と初回 honest fixture1 は31に含むので加算しない。non-test consensus check・再生成一致も成功。[コマンド・raw log・hash・source patch・範囲](../../../evidence/palw/g14-integration-candidate-2026-10-20/README.md#rooted-shared-data-native-adapter-candidate)。8 owner は既存 verified-half Open で占有し、outsider の owner Open が拒否されても共有 query は独立に記録・共有・復元できた。不正計算でも正しく commit されたデータは保持し claim/credit は変更せず、codec/digest 正常でも original-root に合わない改変は復元時に拒否した。

original/context/登録/payment は fixture、lifecycle 未割当・production installer/dispatcher はなし。timely notice・完全な原子的 original 予約・funded/streaming ingress・全敵対分岐/埋め直し/最遅応答・fresh court signer・実 supply・費用/物理校正・依存/終了規則・R-D2-NODE・tag230・実 door<=3,120・T01–T15・node T05/G14 は未完。元の cap/clock/t12 と本番 arming records は変更せず、数値/公開 schema の採択、pre 統合・武装・配備なし。文書検査は同じ imported5件で FAIL、baseline/元資料は未変更。

## Signed shared-stream envelope (2026-10-21)

`b980f5788` 後に独立した署名付き Begin/Part/Finish codec と fixed actor/target slot identity、checked canonical part geometry を実装。実 ML-DSA で全action fieldを束縛し、宣言変更で枠を増やさない。FullIdsは一つの対象でありchunkごとに全列を再提出しない。既存16,384-byte part上限を維持し、署名・固定項目・carrier費用は別途積算する。

最終選択 **73 passed / 0 failed / 0 ignored**（query/frontier15、codec/identity27、同じ最終core executableのnative31）。初回4・途中native31・過去69等は加算しない。non-test consensus check と生成物一致も成功。[raw logs・hash・source patch・範囲](../../../evidence/palw/g14-integration-candidate-2026-10-20/README.md#signed-bounded-shared-stream-envelope-candidate)。

搬送形式だけのcheckpointであり、Ready照合、rooted prefix/chunk、EOF再認証と原子的rollback、再読込/reorg/prune、完全予約・fee・実供給・T05は未接続。現行whole-body内部adapterを本番入口へ出していない。[Draft仕様](../../../rfc/0029-owner-independent-shared-tir-query-data.md#8-signed-bounded-stream-envelope-candidate)とADR-0196へ境界を明記。G14全体は継続中、有効化なし。

## Rooted signed shared-stream (2026-10-21)

`33738ea26` 後に、元preparationのliteral Ready・現在のActive bond・実ML-DSA署名の照合、固定stream行とbounded chunk行、実受理clock、EOF元データ認証・原子的公開を接続。候補key/row17/23・18/24を末尾追加し、旧tag/codec/期限/上限を維持した。chunk本文はcheckpoint間で共有し、prefix全体の再コピーを避ける。全状態/map/indexや他の行のコピー費用がゼロになったとは扱わない。

初回のtiny fixtureでは16,385-byte不正本文が実際のsource上限を超えるという試験側の仮定誤りを検出。狭いケースを残し、既存4096幅実行を正規のoriginal contextへ組み立てた複数Partケースも検証した。さらに小さいchunkのcodec試験が、shared/standard Arcのwire preflightと既存typed heapのalignment余裕の不一致を検出した。hard capやtyped heap側を変えず、preflightを既存の保守的会計へ揃えた。

最終選択 **86 passed / 0 failed / 0 ignored**（wire11 + codec/identity28 + 同じ最終core executableのnative32 + query/frontier15）。non-test consensus checkと生成物一致も成功。[初期失敗・source patch・hash・範囲・重複除外](../../../evidence/palw/g14-integration-candidate-2026-10-20/README.md#rooted-signed-shared-stream-native-adapter)。原子的EOF失敗、公開前prefix、全IDs、改変/欠損拒否、journal undo/replay、carriage再読込、元hard境界・保持を検査した。これはverified-half original/Readyとsynthetic service bookを使うin-memory fixtureであり、有料node ingress・本物のkey rotation・全分岐予約・実供給・fresh duty・pruned IBD・T05/G14完了ではない。武装・pre統合・配備なし。

## Explicit fresh data reply (2026-10-21)

`fa1104eed` 後に、データ共有とliteral signer自身のデータ応答を分ける候補を接続。署名を実受理queryのblock/DAA・元scope・認証済みデータに束縛し、別の受理contextへ移せない。候補key/row19/25は末尾追加。Ready/current bond、データ、署名を実状態から検査し、receiptの再送では時刻を変えない。署名付きEOFだけでは応答済みにせず、default/credit/責任解放にも使わない。

データ未完成の間はresource起点を未確定にし、完成後はquery/Ready/dataの実受理DAAのmax+1を導出する。これは新しい有料義務や応答期限の採択ではない。過去のupload/preparation違反を免責する判断にも使わない。

最終選択 **89 passed / 0 failed / 0 ignored**（G14/activation/codec/identity29 + 同じ最終core executableのnative32 + query17 + wire11）。non-test consensus check・生成物一致も成功。[source hash・生ログ・別受理context/再送/改変拒否の範囲](../../../evidence/palw/g14-integration-candidate-2026-10-20/README.md#explicit-fresh-data-response-native-candidate)。targeted2や過去86等は加算しない。原本/Ready/資金/contextはverified-half fixtureであり、有料node ingress・完全予約・全semantic duty/fresh signer availability・実供給・node T05/T01–T15は未完。武装・pre統合・配備なし。

## D2 node claim-part replacement (2026-10-21)

`11734c11b` の旧ignored試験を明示実行し、新D2 claimへの禁止済みStepLeaf要求がmempoolで拒否されることを再現。既存nodeにはKernelWitness(tag157/158)、ShardCourtAccused(tag38)、非fused再計算(tag159)の経路があり、「代替node受理がすべて未実装」という説明は広すぎた。今回の変更は試験側のみで、daemon/admission/codec/上限/本番パラメーターは変更しない。

2件のD2試験を既存の隔離G14 genesisと通常設定検査へ移し、被告のclaim-only witnessと外部検証者自身のモデル行による実funded carrierの裁定経路を使う。旧StepLeafの実mempool拒否も残して検証する。全Panel Valid共謀、Final保持、返還、DB再起動後の再発見と継続、fresh nodeの全block再生/root/RPC一致を対象にする。

関連試験の初回拡張実行は23成功・13失敗。13件はF02/F03未完成による武装拒否で、旧child fixtureが架空readiness recordで正の設定検査に通るという古い前提だった。このrecord生成を削除し、既存隔離genesisでは通常検査に通り、未隔離genesisでは同じ拒否が残る構成へ修正した。過去のtest-only cost入力を実測・採択値へ昇格させない。

[生ログ・最終結果・source hash・限界](../../../evidence/palw/g14-integration-candidate-2026-10-20/README.md#d2-node-claim-part-replacement-and-isolated-component-fixtures)。これはBase0のnode component試験であり、daemon全体・全family/fused分岐・TIR owner占有/埋め直しT05・物理容量・実door・R-D2最終承認・G14完成ではない。P0/P1/P2と本番武装拒否を維持する。

最終選択は **62 passed / 0 failed / 0 ignored**。同じ最終node executableの関連36件と、G14 gate12・t12 activation5・release identity9。旧ignored D2 2件も通常実行し、13件のchild fixtureも実際の動作まで成功した。初回1成功や途中23成功/13失敗、過去89等は加算しない。変更は試験3ファイルと文書/証拠に限定し、production source・数値・schema・arming recordは未変更。

## Original受付の接続監査 — 2026-10-21

[準備clockの単独調査](rfc26-shared-frontier-volume-2026-10-20.md)に、12,420 DAA下界のsourceが旧main `0f808b81d`であることを明記。既存進捗のX3区別を単独文書にも反映し、現候補へ古い式を適用しない。現行は1pass/1WorkDebit、serial chain＋parallel inclusionの式を使う。work/IO/公開費用や全対話を削除したという意味ではない。

[Original受付と各control/feeの対応監査](rfc26-original-service-admission-map-2026-10-21.md)を追加。original rent、owner Open rent、Convict/最初のWitnessPart rentの実node helperとfoldの関係、shared private primitiveがまだ有料original admissionへ接続されていない点を固定した。全合法graphの証明ではなく、未導出費用のゼロ化・新旧費用の重複加算の両方を防ぐためのsource根拠である。

今回は文書/source監査で、production/test source変更なし。既存clock回帰5＋t12関連26の31件成功。過去62等は加算せず、実容量・実door・完全予約・本番有効化は未達として継続する。

## Serviceの最低material期限を受付検査へ接続 — 2026-10-21

原子的original受付への準備中、service context検査がcustodyをhardまでしか要求していない不足を確認。元frameのmaterialと、hard+元native reorg保持期間の最大をcheckedで求め、TIR/semantic両custodyへ適用する候補を追加した。native writerとcarriage context検査の既存経路を使い、短い署名契約の延長・hard変更・責任解放・default許可は行わない。

[受付監査の修正と限界](rfc26-original-service-admission-map-2026-10-21.md#最低material期限の接続修正)。これは全依存graphの完了ではなく、既存契約から確定できる最低保持条件の接続。完全な有料original予約、全後続義務/裁定/供給/公開ingress、T01–T15と実doorの元条件は継続する。

最終選択は **67 passed / 0 failed / 0 ignored**（native32＋service codec6＋policy3＋gate12＋activation5＋release identity9）。targeted1は32に含まれ、過去31/62/89等は加算しない。non-test consensus release checkも成功。[実コマンド・source/実行file/log hash・fixtureの限界](../../../evidence/palw/g14-integration-candidate-2026-10-20/README.md#minimum-signed-service-material-horizon)。完全なsemantic保持/終了graphの代替とはしない。

## Semantic保持と無応答経済入口 — 2026-10-21

`d4896737c`後に、semantic依存契約/Owner materialがservice保留に含まれない不足と、semantic/history共通の無応答課金入口にservice guardがない不足を修正する候補を追加。元pledge・original bindingを固定し、native Panel更新は許容、TTL/破壊的削除は保留する。無応答のproducer/signer-only両課金を状態変更前に拒否する。客観的反証の裁定経路は別に維持する。

[実装監査・fixtureの限界・次のP0](rfc26-original-service-admission-map-2026-10-21.md#semantic-material署名契約無応答課金の補強候補)。通常courtの`deadlines::sweep_court_deadlines`は別経路でservice guard未接続と確認したため、F02の全面完了とはしない。text Constraint/Prefixのnative originalもtail拒否との対応が未検証。全G14条件を縮小せず継続する。

最終選択 **84 passed / 0 failed / 0 ignored**（同じcore executableのnative32＋public-prefix17、別integration35）。targeted1と過去67/31/62/89等は加算しない。non-test consensus release checkも成功。[中止した旧build・source・hash・検証の限界](../../../evidence/palw/g14-integration-candidate-2026-10-20/README.md#semantic-dependency-retention-and-source-default-guard)。次は未接続の通常court deadline/no-show経路を修正する。F02/F03・G14全体の完成ではない。


## 通常courtの自動無応答処理の保留（`7aa640a8f`後の隔離候補）

`deadlines::sweep_court_deadlines`のladder/phase/backstopと、先行する`sweep_court_close_deadlines`の組立期限に、既存claimの未解決service bindingのguardを追加する。署名済み期限は動かさず、session/group/depositを保持する。呼出し内の索引cursorで保留行を飛ばし、後続の通常処理・孤立行の掃除を続ける。現bookだけで実処理機会・default許可があると扱わない。

[source mapの追加調査](rfc26-original-service-admission-map-2026-10-21.md#通常courtの自動期限処理の保留候補)に、残る別経路を記載した。`write_court(None)`は客観的裁定後を含むsession削除時に残存close groupのdepositを徴収・削除する。配送/収録機会と区別した費用/帰責、未解決責任の精算・material archiveは未完で、自動期限のguardだけをF02/G14完了にしない。正常なfraud closeを全拒否して解決とする変更もしていない。全分岐の原子的有料受付、公開ingress、供給、全依存graph、T01–T15、tag230、doorの実測条件は残る。


今回の最終選択は **98 passed / 0 failed / 0 ignored**。native全体33、public-prefix17、通常court13、codec/policy/G14gate/activation/t12同一性35。targeted1・先行native31・過去の件数は加算しない。non-test consensus release checkもexit0。途中のfixture型誤りによるcompile101は修正前patch/logとともに別記し、試験失敗や成功へ数えない。[検証・hash・正確な範囲](../../../evidence/palw/g14-integration-candidate-2026-10-20/court-service-deadline-artifact.json)。文書検査は同じimport済み欠損5件とbaseline既知43件を残し、新規著者リンク欠損は0。G14全体は未完で、有効化保留を維持する。


## F08終了効果のA案と、service bookの前向き消去の修正候補

`c54ba38fd`後、[RFC-0030](../../../rfc/0030-court-end-effects-and-pending-liability-archive.md) / [ADR-0197](../../../adr/0197-court-end-effects-and-pending-liability-archive.md)に、RFC-0026 §4.3の全6終了理由と完成assembly不整合のfield別効果を具体化した。利用者は2026-10-21にA（裁定と未決責任のarchiveを分離）の詳細仕様化・隔離実装を指示した。本番採択・pre統合・武装・配備ではない。archive row、全金融hold移行、解除validatorと有料original全分岐graphはまだ実装していない。

[ソース調査](../../../evidence/palw/g14-integration-candidate-2026-10-20/court-end-effects-source-audit.json)では、現close depositはdeclaration時に送金されたescrowではなく、残存collateralから後で徴収する額であることを確認した。archiveに額を保存しただけでは資金が確保されたとはならない。`offence_attribution_active`時の中立voidも、共通session削除writerを通り残存groupの徴収/削除へ到達する。正常なfraudの成立を止めて回避しない。

方式選択に依存しない修正として、shared context/duty clockの作成前でも、native service writerが未解決bookの削除・型置換・空bookへの置換を拒否する。受理済みのprofile/容量、original binding/packetの約束、既存供給履歴と観測clockを保存する。過去に飛ばされたslotへの供給の後付けを拒否し、assignmentを動かせるのは経過済み未供給slotから観測clock後への修復だけとする。新claimの将来予約、前向き観測、従来の修復とjournal undo/replayを禁止する規則ではない。既存の署名期限、実供給の認証、defaultの許可、terminal解消・pruning条件を新たに採択したものではない。


このbook保持修正の最終選択も **98 passed / 0 failed / 0 ignored**（native33 + public-prefix17 + 通常court13 + service/policy/G14/activation/t12同一性35）。targeted1と前ターン98は重複として加算しない。non-test consensus release checkはexit0。追加fixtureは構造/context-validな置換をnative writerで拒否する検査であり、実署名のoriginal受付・実供給やarchiveのE01–E10の成功ではない。[source/実行file/log/検証範囲](../../../evidence/palw/g14-integration-candidate-2026-10-20/service-book-retention-artifact.json)。文書検査は同じimport済み欠損5件・baseline既知43件を残す。次はAの新archive、実際の金融hold/証拠移行、qualified settlementと全分岐の有料受付を接続する。G14未完・有効化保留を維持する。


## F08 A案：rooted archiveの部分実装

親`494361118`。利用者のA判断に従い、[RFC-0030 §8](../../../rfc/0030-court-end-effects-and-pending-liability-archive.md#8-a案の隔離実装--2026-10-21)のprivate adapterを追加。serviceに束縛されたcourtの終了時に、元session・phase・残存両group・完成raw bytes・全decoded TIR終了証拠をarchiveへ原子的に移す。反対側の未完groupを自動徴収せず、完成不整合側の実deposit debitと正常完成groupのfulfilled状態を区別する。未決名目額を既存accuser exposureとexit holdへ接続する。未接続/関連causeだけの終了では元court stakeを保持し、未知の裁定/pointを作らない。

新private key 20 / row 26を末尾へ追加し、旧X3/service/tag230の意味を変更しない。namespace identityは校正済み有料profileではない。全金融入口への原資予約、元署名全文、終了後の未完group提出とqualified settlement/pruning、全分岐original受付・実供給・公開ingressは未実装。保留の継続だけをF08/G14完成としない。

新5試験は小型verified-half fixtureで、二つのcloseはlocal legacy v2 adjudicatorを使う。nodeのv5/held-regime/signature受付を通った試験ではない。native raw完成経路、分割TIR証拠全体、carriage・heap/wire・期限後保持・全undo/replay/root一致を対象とする。関連causeのstake検査はread-only counterexampleで、偽の受理済みnode recordではない。元service cost/署名/宣言設置のfixture入力を実料金・資金確保・全最大phaseの測定へ昇格させない。

[終了後提出経路のsource調査](../../../evidence/palw/g14-integration-candidate-2026-10-20/court-end-archive-followup-audit.json)では、旧CourtCloseChunkがactive groupを前提とし、完成時にv5と実CourtParams/各fork条件を検査することを確認。将来のresolverは元の適用規則に束縛し、仮params・v2への降格・任意row上書きで解除しない。T01–T15、全owner占有/埋め直し、実door、全金融/資料/依存graph、R-D2/tag230の残件と有効化保留を維持する。


最終選択は **103 passed / 0 failed / 0 ignored**（同じ最終core executableのnative38＋public-prefix17＋通常court13、integration35）。targeted5は38に含み、途中103・初回1成功/2失敗・過去98等を加算しない。non-test consensus release checkもexit0。途中のcompile101/初回2失敗は当時のsource patch/hash/logを保存し、全4版をpatchから再構成してhash一致を確認した。[最終実行・source/実行file/log hashとfixtureの限界](../../../evidence/palw/g14-integration-candidate-2026-10-20/court-end-archive-artifact.json)。文書検査は同じimport済み欠損5件によりFAIL/exit1で、baseline既知43件も残す。今回の新規リンク欠損は0。過去service-book-retention artifactのdocs exit0は元FAILログと不整合であり、今回の直接実行でexit1と訂正する（元ログは変更しない）。G14未完・有効化保留を維持し、次は元規則を束縛した未決解消validatorと全分岐の原子的有料受付へ進む。


## F08：元declaration署名とnative v5入力の保存

親`e2e367e98`。[RFC-0030 §9](../../../rfc/0030-court-end-effects-and-pending-liability-archive.md#9-宣言時の署名native裁定入力の保存候補)のprivate receiptをnative CourtCloseDeclared経路へ接続する。元object/署名/公開鍵、宣言時のsession/embedded・別TIR phase、実point、native processor由来のv5入力を保存する。署名再検査と現group契約の一致、forward immutable、実declarationイベントとextrasの一致により、過去の署名へ任意のnative条件を後付けしない。入力のない旧groupは未被覆のまま、`rules=None`はdefault paramsの代用ではない。

新key 21 / row 27は末尾の隔離候補で、archive20/26とX3/service/tag230を変えない。native extrasの入力追加はpresetのBorsh/identity/feeやfenceの変更ではない。資料を保存するだけで新配送義務・無応答default・担保解除・pruningを許可しない。元の署名はnative fork条件への追加署名ではなく、native条件は別の合意受理履歴として保持する。

全料金/資本・original graph・公開node ingress、終了後v5再検査とqualified settlement、最大phase/family、T01–T15と実doorは引き続き未完。延期中の機能は無効を維持する。


最終選択 **104 passed / 0 failed / 0 ignored**（native39＋public-prefix17＋通常court13＋integration35）。実署名の新試験1件は39に含まれ、過去103やcompile checkを加算しない。core testsの型検査と最終non-test consensus checkもexit0。途中のcompile101二件は実行試験0件としてsource patch/hash/logを保存した。[実行・source/実行file/log hash・正確な限界](../../../evidence/palw/g14-integration-candidate-2026-10-20/court-close-receipt-artifact.json)。文書検査は同じimport済み欠損5件でFAIL/exit1、baseline既知43件を残し、新規著者リンク欠損は0。元署名とnative条件の保持は前進したが、終了後v5検査・金融精算・全分岐original予約・G14の完成を意味しない。


## F08：保存した入力によるprivate v5再検査

親`c153ecc02`。[RFC-0030 §10](../../../rfc/0030-court-end-effects-and-pending-liability-archive.md#10-保存した入力によるprivate-v5再検査候補)に、元の署名/contract/chunk prefixと完成raw bytesを照合し、保存したnative入力とsession/phaseで既存v5を呼ぶprivate verifierを追加した。現在のextrasやv2へfallbackせず、実結果と署名verdictの不一致を拒否する。非Borsh・非Cloneの結果は元state root/session/side/raw group/decoded closeへ束縛する。元state・担保・credit・journalは変更せず、解除/精算/defaultを許可しない。

wire CPU予算/shape算術・OOMをNativeResourceとして伝播し、切断された構造入力と区別するため、最初の自分のrelease buildを実PID確認後に明示SIGINTした（exit130、実行試験0）。timeoutで停止と推測した再起動ではない。中間source/patch/logを保存し、修正後に型検査とrelease試験を行った。原本ログを変更していない。

最終選択 **105 passed / 0 failed / 0 ignored**（native40＋public-prefix17＋通常court13＋integration35）。新しいparametric試験1件は40に含み、途中compile checkや過去104/103を加算しない。小型の正常/不正TIR cone、held flag false/true、正誤の署名verdictの8組、実署名、native宣言/prefix/終了、TTL後の再検査・carriage/全undo/replay、実reader予算エラーを検証した。held/fused phaseや最大全familyの検証ではなく、原本/資金/serviceはfixtureの前提を残す。non-test consensus release checkとcore tests型検査もexit0。[実行・source/実行file/log hash・詳細な限界](../../../evidence/palw/g14-integration-candidate-2026-10-20/archive-v5-artifact.json)。

現在は検査view用に全state copyをquote/cloneしており、全有料graphの追加費用・物理容量は未校正である。per-claimの予約上限を閉じるため、借用したsession/phaseを既存v5の共通kernelへ渡し、global state copyを避ける検討が必要。公開受付で適用する時点/規則、金融精算・pruning、全actorの資本、全分岐original予約・実供給、P2局所化/準備/容量、T01–T15・R-D2/tag230・実doorは未完。G14の完成や武装/統合/配備ではない。


## Archive再検査の全state copyを除去する候補

親`54c0081d6`。公開native v5と保存済みrecordのprivate再検査を同じkernelへ接続し、archive経路のglobal state/session/phase cloneと一時active行の設置を除く。既存のcost gateをstate参照前に維持し、別phaseの取得先3箇所以外のkernel本文は変更前とbyte単位で一致する。出力group・署名message・assembly・decodeのquoteは残す。[RFC-0030 §11](../../../rfc/0030-court-end-effects-and-pending-liability-archive.md#11-借用recordを使う共通v5-kernel)を参照。

元rootのhash、署名、内側のlegacy allocation/CPUと物理容量はなお全profileの未校正費用。全original分岐の有料受付、公開ingress、実供給、金融精算/解除/pruningを実装したという意味ではない。最終検証の結果と正確な範囲は[artifact](../../../evidence/palw/g14-integration-candidate-2026-10-20/borrowed-v5-artifact.json)へ保存する。G14全体・武装・統合・配備は未完/保留を維持する。

Final borrowed-v5 selection: 134 unique passes (native40 + public-prefix17 + ordinary13 + kernel23 + integration41). Targeted1 overlaps native40; historical105 is excluded. Six F7 cases use a toy corpus and do not demonstrate measured real-model door or funded public archive admission.

Core test type-check and non-test consensus release check both exited 0. Documentation checking remains FAIL/exit1 with the same five imported missing references and43 baseline-known occurrences; no newly authored missing link or baseline update. All three changed runtime files were reconstructed from the parent and saved patch and matched SHA-256.
## 終了後の署名済みmaterial追加候補

親`2fcbcac6a`。[RFC-0030 §12](../../../rfc/0030-court-end-effects-and-pending-liability-archive.md#12-private-append-of-precommitted-archive-material)のprivate内部経路で、元宣言が指定した未受理chunkだけをarchiveへ追加する。署名・index・size・digestを確認し、元scope/session/phase/終了証拠・受理済みbytes・期限・stake/deposit・徴収済み/fulfilled状態を変えない。既存journal writerの前向き更新を、materialの追加に限定して許可する。wire/row/key/schemaの変更はない。

完成しても履行済みや適時応答と認定せず、名目責任・担保・claimを維持する。不整合な宣言の実bytesも保存し、既存private v5再検査の拒否結果と区別する。実費予約を伴う公開入口、公開late適用時点、適格な金融精算・解除・pruning、全original graph・実供給・P2・T01–T15/実doorは未完。単なる無期限holdを完成にせず、これらへの後続接続に必要な実materialを保存する段階である。
今回の最終選択は134件（native40＋prefix17＋通常court13＋kernel23＋integration41）。既存8組の試験を拡張したため、新規test件数として加算しない。targeted1・過去134等も除外する。[実行コマンド・hash・範囲](../../../evidence/palw/g14-integration-candidate-2026-10-20/archive-material-artifact.json)に保存する。最初の型検査は試験引数の参照型不一致によりexit101で、修正前source/patch/logを別保存した。実行試験0である。
Non-test consensus release check exited0. The four changed runtime sources were reconstructed from parent plus patch and matched SHA-256. Documentation remains FAIL/exit1 with the same five imported missing references and43 baseline-known occurrences; no new authored missing reference or baseline update. Full G14 and activation remain incomplete.
