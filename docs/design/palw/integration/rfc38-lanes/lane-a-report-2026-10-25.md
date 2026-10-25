# Lane A 報告 — 資源所有権と生成前予約（2026-10-25、後任）

branch `claude/rfc38-lane-a2`（base `fdd468a5b`）。前任 WIP（`rfc38-lane-a`、base `14fc6e997`、未 commit）を取り込み、
両 base 間で対象ファイルに差分が無いことを確認したうえで同一内容を複写した。push・deploy・activation 変更なし。公開 activation は None のまま。

## 1. 閉じた要件（この差分）

| 要件 | 変更前の実 caller | 実装 |
|---|---|---|
| RFC-0006 / RFC-0018 §7: Gate C certification sweep の aux-136 row snapshot | `sweep_security_certification_v1` が kernel-route aux 136 範囲全体を `Vec<(Vec<u8>,Vec<u8>)>` へ deep clone（予約なし） | snapshot 構築前に実 row の key/bytes 長から quote（`exact::<u8>` × 2/row、外側 Vec の成長 `grown` × 2）し、`admit_native_write_v1` で transition の construction owner へ admit。owner は builder drop まで保持（snapshot の最後の使用より後）。拒否は typed `NativeResource`（再試行可能な local 資源不足）で、certification の意味的結果にしない。追跡無効（全 shipped preset）では従来どおり byte 同一 |
| RFC-0016 §11 列 7（audit 16 PARTIAL）: 容量・不足 report | `capacity_per_span`/`capacity_shortfall_per_span` が常時 `None` | admission と同じ attested/water-filled 読み（`rfc0016_capacity_read_v1`）をモデルの非 base class で合算。class ごとの不足（`demand − capacity`）を加算（兄弟 class へ移さない）。容量 fence 未満または registry fold 無し = `None`（UNKNOWN）、未 attest class = `Some(0)`（実ゼロ）、overflow = `None`（切り詰めない）。容量 fence 以後は class を持つ全モデルに行を出す（rights・block が無くても容量を報告）。fence 未満の行集合は不変。RPC JSON に `capacityPerSpan`/`capacityShortfallPerSpan`（`null` と `"0"` を区別） |

変更ファイル（leaf）: `consensus/core/src/palw_security_certification_sweep_workspace_v2.rs`（新規）、`palw_security_certification_fold_v1.rs`、
`palw_rfc0016_fold_v1.rs`、`palw_rfc0016_v1/report.rs`、試験 `palw_state_v2/tests/security_certification_sweep_workspace_v2.rs`（新規）、
`palw_state_v2/tests/rfc0016_capacity_attestation_v1.rs`。中央: `palw_state_v2/tests.rs` の mod 登録 1 行のみ（`central:` commit）。
wire tag・既存 ID・DB schema は変更なし。

## 2. 境界 API（A → B・C・D・F）

- resource vector: `NativeWriteQuoteV1`（`palw_state_v2/native_writers.rs`）→ `TirNativeRequestsV1`。上限は `NativeSnapshotResourceLimitsV1 { memory_bytes, database_bytes }`。
- 予約 token の所有者: `TransitionBuilder.native_reservation`（`NativeTransitionReservationV1::for_checker_v1(&Arc<NativeDemandCheckerV1>)`）。caller scratch は `admit_native_write_v1(quote)` で同 owner へ。
- commit/abort: admit は全量か無（拒否時 `outstanding()==(0,0)`、state・journal 不変）。成功時は builder drop で解放。永続化しないため再起動で残留しない。panic も drop で解放。
- 最終解放: builder drop。snapshot の最後の使用は sweep loop 終端で、それより後。
- B calendar への接続点: B 報告のとおり一般 resource-vector calendar 型は未定義。提案: key = `PalwClaimLocalWorkRefV1`、値 = 上記 `TirNativeRequestsV1` と deadline（session `deadline_daa`／group `assembly_deadline_daa`）。型の定義は統括判断（中央）。本差分では接続していない。

## 3. 試験証拠

- `CARGO_INCREMENTAL=0 ~/Downloads/MISAKA-wt-b/buildslot.sh cargo test --offline -p kaspa-consensus-core --lib -- security_certification rfc0016 rfc16_`
- 結果: exit 0、`51 passed; 0 failed; 0 ignored; 5168 filtered out`（既定 feature）。途中 1 回 FAILED（report 試験が fold 無しの行存在を前提）→ 行集合規則と試験を修正して再実行
- sweep: 上限 0 と Q−1 で拒否（state・state_root・journal・row 不変、予約 0）→ 上限 Q で admit・保持 → 十分な上限で実 sweep が 2 row を retire、`outstanding ≥ Q` を drop まで保持 → drop で 0。追跡無効と byte 同一、delta apply/revert 一致、quote は row bytes に比例、route 無し／fence 未満は quote 0。
- report: fence 以後 fold 有りで admission の読みと一致、未 attest モデル `Some(0)`、fold 無し・fence 未満は `None`、JSON で `null`/`"0"` を区別、overflow は `None`。
- fixture 上の PASS であり実 node campaign ではない。全モデル費用境界（実 row 数・実 model の負荷）は実測範囲外。

## 4. 残件

- CODE: sweep loop 内の per-row transient（decode した `draws`/`attestations`、claim clone、draw pool）は未 quote（明記）。caller 対応表の残り未予約経路（GDN disclosure 認証前確保等）は未着手。B calendar 型への接続（依存: 統括の型定義）。
- TEST: 実 node での上限拒否 → 再試行。
- EXTERNAL / POLICY: 実 limit 値（`memory_bytes`）の決定は値決定待ち。

この差分で閉じたのは上表 2 件のみ。RFC-0006・0016 全体は未完了。
