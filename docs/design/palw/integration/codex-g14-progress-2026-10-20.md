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
