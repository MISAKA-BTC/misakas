# ADR-0204 — IBD の有界 authenticated row stream と世代番号付き保存

Status: **Adopted / 実装・検証中、配備未承認** (2026-10-30、ユーザー明示採択。初稿 2026-10-22)。[RFC-0037](../rfc/0037-ibd-bounded-row-stream-and-storage-generations.md)。
設計: [ibd-generations.md](../design/palw/storage/ibd-generations.md)。2026-10-30 にユーザーが採択し、新 P2P version の割当てと実装を明示承認した。公開配備・既定有効化は別判断とする。

## 問題

pruning point の PALW state は全量受信・全量 decode の後でしか header の root と照合されず、一時領域とメモリが
state の大きさに比例する。G14 storage の上限(pruned node ≤ 30 GB、IBD spill 384 MiB × 2)の下では、受信を
有界にし、偽の state を途中で止め、検証済みのものだけを原子的に昇格させる必要がある。

## 決定

1. **consensus は変えない。** state root(ADR-0043 §2 の flat commitment)をそのまま使い、preimage を先に照合し、
   collection ごとに逐次 hash して preimage の該当 64 byte と照合する row stream を P2P 転送形式として追加する。
2. stream の照合は「早期中断と有界 disk」のためで、**最終判定は既存の import ゲート**(`restore_native_carriage_v1`
   + 全 root 再計算 + `assert_internal_consistency*`)のまま。root、または root に一致する stream だけでは検証済みに
   しない。
3. 新 message は新 protocol version の交渉下でのみ使い、旧 peer には従来の 106/107 carriage 転送を続ける。
4. node-local の世代保存(最大 2 世代、fsync + rename の journal、`VERIFIED` marker、undo log、切替をまたぐ巻き戻し)を
   受け皿とする。これは node-local で、本 ADR の採択を待たず既定 off / 既定不変で実装済み。

## 代替案

| 案 | 内容 | 不採用(または後回し)の理由 |
| --- | --- | --- |
| A(本提案) | flat root のまま preimage 先行・collection 単位検証 | consensus 不変。粒度は collection 単位(行単位ではない) |
| B | 木構造の state commitment で行単位の membership/non-membership proof | consensus 変更(新 state-root version)。state lane の RFC で扱う。採択されれば A の第 1 段を置き換えられる |
| C | 現行のまま(全量受信後に照合) | 上限内で state の成長に追従できない |

## 影響

- consensus fingerprint・header・fork choice・Panel 抽選: 変更なし。
- P2P: 新 message(休眠)。旧 peer の動作は変わらない。
- 帰責: preimage/collection の不一致は peer、容量・I/O は local(停止・再試行、block を無効にしない)。

## 承認の範囲

求めるのは A の方式の採択可否。protocol version 番号、message 名、chunk 大きさ、移行期間、phase 2(全体検査の
bounded-memory 化)の採択は含まない。

## 記録

- 2026-10-22: 受信側 verifier(`consensus/core/src/palw_row_stream_v1.rs`)と node-local 世代保存
  (`database/src/storage_generations/`)を実装・テスト。wire 未接続。番号: ADR-0204 / RFC-0037
  (ADR-0203 / RFC-0036 は同日の state lane(authenticated state)用に空けた)。


## 採択記録 — 2026-10-30

ユーザーの明示採択により protocol **109** を割り当て、RFC-0037 の実装・検証を進める。
`--palw-row-stream-ibd` は既定 off、通常の広告 version は 108 のまま。両端の opt-in と version 交渉後にのみ
codec 2 / current generation 1 を既存の最大 4 MiB の chunk で運ぶ。総量 384 MiB と P2P 上限は変更しない。
原 state root と既存 whole-state import を維持し、最終 import の前には保存世代を検証済みに昇格しない。
実装の接続・帰責・phase 1 の suffix は [RFC-0037 の採択追補](../rfc/0037-ibd-bounded-row-stream-and-storage-generations.md) に記録する。
採択は公開配備・有効化・受入試験の完了を意味しない。
