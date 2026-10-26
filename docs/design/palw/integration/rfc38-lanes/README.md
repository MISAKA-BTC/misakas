# RFC01〜38 並列実装 第 1 巡 — 統括記録（Claude 0、2026-10-25）

指示書: [rfc01-38-claude-parallel-instructions-2026-10-25.md](../rfc01-38-claude-parallel-instructions-2026-10-25.md)。この記録は RFC 完了・activation・campaign 合格を宣言しない。全 38 本の完了件数は再監査まで更新しない（監査 snapshot `rfc-implementation-audit-2026-10-25-head1257` は改変していない）。

## 基点と統合 branch

- 開始 SHA: `14fc6e997`（Codex `codex/rfc06-12-continuation` の確定 HEAD、指示書を含む）。統合 branch: `claude/rfc38-integration`。
- Codex の確定 commit `a4973592d`（署名 provider の daemon checker→native funded challenge 受入）と `78277650f`（公開履歴 segment 2 の記録）を取り込んだ（`a7c499dd7`・`5487efe91`）。Codex の未 commit 差分（challenge fold・held DA・mesh fold・state chunk map・`palw_step`・GDN held filer・settlement_once/paid_claim_reader 試験）は取り込まず、全レーンに編集禁止として伝えた。
- 公開 activation は None のまま。fence・Params・wire tag・DB schema の変更はない。push・deploy はしていない。

## 運用上の記録

- session の worktree guard は、統括 session から他 worktree への Write/Edit を拒否する。統括が作った `rfc38-lane-*` worktree では Lane B が Bash 経由で 2 commit（`bab113757`・`e7fa48871`）と Lane A が未 commit WIP を作っており、guard の迂回にあたる。B の差分は統括が読んで取り込み、A の WIP は後任が隔離 worktree へ取り込んで再検証した。以後の全レーンは Agent の隔離 worktree で実行した。
- 隔離 worktree は `d2214e057` から作られるため、各レーンが統合 branch の HEAD へ reset してから作業した。

## レーン別の取り込み

| レーン | 統合 commit | 閉じた要件（この差分の範囲だけ） | 報告 |
|---|---|---|---|
| B graph | `044a8d1b2`・`30e9dab86` | RFC23 §4.1 / RFC30 §1–4: 未完了の court close group を checkpoint liability material へ。RFC26 readiness の typed 入力境界 `AllBranchTermsSourceV1`（Ready 経路なし） | [lane-b](lane-b-report-2026-10-25.md) |
| G2 refusal matrix | `dad992ee0` | RFC03/04/22 延期 14 object の fold-level 拒否 matrix（全出荷 ruleset・複数 DAA・replay 決定性）。実 process の RPC/restart/resync 拒否は未 | [lane-g](lane-g-report-2026-10-25.md) |
| C funding | `ba7075613`・`be6671afd` | RFC35 A escrow の保存則・exactly-once・資金不足時無変更・restart/reorg を native UTXO 台帳で（試験専用 schedule、本番は `Err(Schedule)` のまま） | [lane-c](lane-c-report-2026-10-25.md) |
| D verifier | `44124edfe` | RFC07/14: 署名付き取得拒否は回答義務（funded ChallengeCurrentV4）で conviction にならない陰性対照。4 adapter 共通の typed 取得報告（request/byte/workspace） | [lane-d](lane-d-report-2026-10-25.md) |
| E output | `1021e158f` | accounting seat の carrier 照合で fold が落とす同一 claim decoy を除外（fold-admissible が 2 つなら fail closed のまま）。RFC 要件の閉鎖はなし | [lane-e](lane-e-report-2026-10-25.md) |
| F material | `9df6e2c8f`・`93ecd9b5f` | RFC37 node-local row stream: `finish_covering` の欠落・過剰・重複拒否、`palw_resume_row_stream_v1` の spill 再照合・byte 再計上 | [lane-f](lane-f-report-2026-10-25.md) |
| G3 gates/RFC12 | `2d858be9f`・`e8b928ee2`・`5cf06475e` | 15 gate runner の未入力 cap（K3=EXTERNAL、native39=POLICY）・scenario ledger の fsync 保存・`--aggregate-root` 再集計。`eth_call`/`eth_estimateGas` の safe/finalized 不在を server error に。15 gate は全 UNKNOWN | [lane-g3](lane-g3-report-2026-10-25.md) |
| A reservation | `9d497c822`・`ff8055629`・`4bfa585ae`（central: mod 登録 1 行）・`dc399aa42` | RFC06: Gate C sweep の aux-136 snapshot を構築前 quote→builder 所有→drop 解放。RFC16 §11 容量・不足 report を attested 読みから（未知 `None` / 実ゼロ `Some(0)` を区別） | [lane-a](lane-a-report-2026-10-25.md) |

## 横断試験

同一 source（`dc399aa42`）・同一 target で 1 回実行した結果を下に記す。fixture／in-process の PASS であり、実 node campaign・15 gate の PASS には数えない。

全て `buildslot.sh` 経由、`CARGO_INCREMENTAL=0`、feature `kaspad/private-integration-config,kaspa-consensus/private-integration-config`（node 側）。

| 段 | command（要約） | 結果 |
|---|---|---|
| 1 | `cargo test -p kaspa-consensus-core --lib -- rfc23s2_checkpoint original_aggregate_draft funded_escrow palw_row_stream_v1 security_certification rfc0016 rfc16_` | exit 0、91 passed / 0 failed |
| 2 | `cargo test -p kaspa-consensus-core --test palw_deferred_feature_fold_matrix --test ibd_row_stream_resume --test ibd_row_stream_generations` | exit 0、19 passed / 0 failed |
| 3 | `cargo test -p kaspad -p kaspa-consensus -p misaka-palw-sdk --lib -- accounting_seat_ state_selector provider_reader_v4 kernel_paid_checker` | 既定 TMPDIR（`/var/folders`、symlink 配下）では 11 failed（"kernel paid outbox traverses a symlink"、試験環境起因）。`TMPDIR=/private/tmp` で再実行し exit 0、47 passed / 0 failed |
| 4 | Lane D の ignored parent 2 本（`rfc07_signed_socket_*`、`MISAKA_SIGNED_PROVIDER_CHECKER_BIN_V4` に同一 source の kaspad lib test binary） | 実行中（第 1 回は filter 先の crate 違いで 0 件実行、受入に数えない） |
| 5 | `python3 -m unittest`（`scripts/drills`） | OK |
| 6 | `git diff --check 14fc6e997 HEAD`、`python3 docs/maintenance/check_docs.py` | 差分検査 clean、docs PASS（既存 baseline の不足 reference は残存・増減なし） |

log: `scratchpad/xlane/`（session 一時領域、永続証拠ではない）。

## 決定待ち（ユーザー判断・採択が必要）

| ID | 内容 | 出典レーン | 塞いでいるもの |
|---|---|---|---|
| Q1 | claim record に fence 付き carrier txid 欄（または rooted claim→carrier map）を追加するか | E | actual winning carrier index、B の `inputs` 充填 |
| Q2 | plain worker が運ぶ context identity の policy | E | worker/SDK の native handshake |
| Q3 | RFC35 schedule の値（D-C1）と payee 精算（D-C2、hard deadline 後に payer 返還が未払い dues より優先される現挙動の意図） | C | `AllBranchTermsSourceV1` の Ok 型、本番 escrow 経路 |
| Q4 | RFC33 fee/3MSK slash（D-C3）、RFC34 S1（D-C4）、RFC27（D-C5） | C・D | ChallengeCurrentV4 の fee・default slash の出典 |
| Q5 | native39 → RFC12 写像（D-C6） | C・G3 | Safe/Finalized の陽性結果、bridge/market fill、gate 3 本 |
| Q6 | RFC09 provider discovery と post-checkpoint L2 検証方式（RFC 上 DESIGN_GAP） | G3 | RFC09 実装 |
| Q7 | RFC37 row stream の wire 採択・protocol 番号、typed kernel material の P2P serve/request message | F | 実 P2P 受入 |
| Q8 | RFC25 の bounded 混雑 extension policy | B | 一般 calendar の extension |
| Q9 | 15 gate と入力の割当（G3 の保守的割当）の確認、凍結 campaign plan と固定 candidate binary | G3 | 実複数 process campaign |

## 統括の CODE 残件（決定不要、次巡の候補）

- 一般 resource-vector calendar 型を `PalwClaimLocalWorkRefV1` キーで中央定義し、A の `NativeWriteQuoteV1`/`TransitionBuilder.native_reservation` と B の deadline を接続する。
- sweep loop 内の row ごとの一時確保の見積もり、GDN disclosure の認証前確保など未予約経路（A）。
- checker の CPU/RSS 予約 vector と court 費用の報告（A・D）、Program 以外 3 target の実 socket 陰性対照、DA default の native 試験（D）。
- 約 80 collection の codec 表と serve 側の割当、`finish_covering` の required 一覧を表から導出（F）。
- completion/cancel/retry・多 process 重複出力防止、RFC18 T0f/T1e/T3c/T3d の最上位 process 試験（E）。
- virtual processor 経由の決済試験のための central 試験注入点（C）。
