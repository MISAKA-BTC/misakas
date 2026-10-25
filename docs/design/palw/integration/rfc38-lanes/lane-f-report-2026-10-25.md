# Lane F 報告 — material・provider lifecycle・streaming import（2026-10-25）

開始 SHA `5487efe91`（統合 branch `claude/rfc38-integration`）、branch `claude/rfc38-lane-f`。push・deploy・activation 変更なし。公開 activation は None のまま。計算の暗号学的証明は不使用（ハッシュは同一性用途のみ）。モデル二階層は変更なし。中央ファイル（`palw_state_v2.rs`、Params、crate root、Cargo.toml、wire/DB registry）は無変更のため `central:` commit はない。Codex 編集中の file には触れていない。

## 1. 対応表: collection → serve/codec/typed assembly/NativeDemand → import/resume → restart/reorg/prune

| 段 | 現実装（実在 caller） | 閉じている範囲 | 閉じていない範囲 |
|---|---|---|---|
| collection の root 検証（RFC37 §2） | `palw_row_stream_v1.rs` の `PalwRowStreamVerifierV1`（preimage 先行、collection ごとに `end_collection` で照合、byte 予算、重複 label/offset 拒否） | 試験 `consensus/core/tests/ibd_row_stream_generations.rs` | caller は試験のみ。本番 caller なし（wire 未接続） |
| spill の読み戻し | `palw_visit_row_spill_v1`（有界、row ごと callback） | 単体試験 | 本番 caller なし |
| serve（collection → row 列） | なし（`state_root_preimage_located_v1` は collection を root としてだけ書く。row を label/offset 付きで出す serve 側は未実装） | — | 約 80 の collection と fence 依存 block の codec 表（RFC37 §5.1）。**CODE / POLICY(採択)** |
| wire | 未登録（RFC37 §3: 新 message は休眠、protocol version 未割当） | — | proto message・protocol 番号・移行。**POLICY（RFC37 未採択）** |
| typed assembly | 既存 `restore_native_carriage_v1`（全 root 再計算つき）。row spill からの組立は未実装 | 現行 carriage 経路は閉じている | spill → typed carriage。**CODE** |
| NativeDemand | `palw_native_resource_v1`（`NativeSnapshotInventoryV1`、`reserve_native_install_in_database_v1`）は現行 carriage 経路に接続済み | 現行経路 | row stream は未接続（RFC37 §2 末尾が明記）。**CODE** |
| import | 現行: `protocol/flows/src/ibd/flow.rs` → `import_pruning_point_palw_snapshot`（`processor.rs`）。declaration は重複・未知・欠落・identity を `palw_validate_snapshot_declarations_v1_impl`（`processor/palw.rs`）が protocol 107 以上（`complete`）で明示拒否。107 未満は legacy filtering（互換）。chunk 側は `SnapshotReceiver::accept` が point/root/total/count/index/size/hash を検査 | 現行 carriage + class declaration | row stream 経路は別 |
| resume | 現行 106/107: `ResumeCache`（同 point の中断 receiver を 1 つ保持、新 peer でも同一 stream のみ、TTL を延長しない）、試験 `an_interrupted_stream_resumes_on_an_identical_new_peer...` | 現行 carriage | row stream の resume は未実装だった → 本差分 F-2 |
| restart / reorg / prune（typed material） | 実 node e2e: `g14_kernel_route_e2e/material_typed_ingress_v4.rs`（提供・cold restore）、`material_typed_reorg_v4.rs`（replacement rollback・restart で material 保持・同一 carrier 再受理）、`material_gc_v4.rs`（署名付 replacement と GC release）、`material_typed_pruning_v4.rs`、`material_typed_terminal_v4.rs`（WAL replay）、`material_typed_source_manifest_v4.rs` + `material_typed_pruned_import_v1.rs`（native pruned import → cold reopen） | native consensus 内の provider/replacement/reorg/prune | 下記 P2P |
| 実 P2P での pruned import | `material_typed_pruned_wire_v1.rs`: 実 TCP + 本番 Factory staging/Ctl/Manager で retained-source の pruned IBD と 1 block relay | **PALW state carriage（class declaration を含む）の P2P 受入**。daemon の Args 起動は含まない（同 file 冒頭が明記） | 下記 |

### 実 P2P でまだ閉じていない経路（native 直接 import を代用にしない）
1. typed kernel material（`PaidClaimMaterialWireV2`、provider の公開・replacement）には P2P の serve/request message がない。`protocol/` 配下に参照が無い（grep で確認）。P2P にあるのは base0/FP 用の `PalwTraceMaterialBroadcast` / `PalwMaterialRequest` のみ。typed material の試験は `send_saved_carrier` による node 内 submit で、P2P 受入ではない。**CODE + POLICY（wire 採択）**
2. RFC37 の row stream は wire 未登録。F-2 を含め、現時点の caller は試験のみ。
3. 100k 規模・OS crash・複数 process は G の campaign 範囲で、本差分では実行していない。

semantic table は borrowed 供給で material target 対象外という境界を維持した（変更していない）。

## 2. 実装

### F-1 RFC37 §2.1/§5.1: 欠落・過剰・位置違い collection の明示拒否（coverage）
- 変更前: `finish()` は「名指しされた collection」が一致することしか見ず、`claims` を丸ごと省いた stream も通った（省略は whole-state gate まで分からない）。
- 実装: `PalwRowStreamVerifierV1::finish_covering(&[(label, preimage offset)])`。closed 済み集合が required と完全一致しなければ型付きで拒否: `MissingCollection` / `UnexpectedCollection` / `CollectionOffsetDiffers` / `RequiredLayoutDuplicate`。required 一覧の供給元は codec 表（未実装、上表）で、本関数は表の内容を創作しない。whole-state gate の代替ではなく、検証済みとも見なさない。
- ファイル: `consensus/core/src/palw_row_stream_v1/coverage.rs`（新規 leaf）、`palw_row_stream_v1.rs`（`mod` 登録と error variant 追加のみ）。

### F-2 RFC37 §5.3: collection 境界での resume（認証・順序・費用計上を飛ばさない）
- 実装: `palw_resume_row_stream_v1(expected_root, preimage, spill, limits, max_row_bytes, projection_of, open_sink)`。
  - preimage を header 由来 root に再照合（別世代は `PreimageDoesNotMatchRoot`）。
  - 保存済み spill を同じ rooting 規則で replay し、完了済み collection を全て preimage に再照合（spill は offset を持たないので、root が preimage の未使用位置に存在することで位置を決める）。改変・別世代は `ResumeSpill`、重複 label は `ResumeSpill`。
  - replay した row を byte 予算に再計上（`ByteBudget`）。再開後の合計は中断なしの場合の上限を超えない。
  - 末尾の不完全 collection（`UnexpectedEof`）だけを最後の完全境界まで切り捨て（`torn_tail`）。その他の破損は spill 全体を拒否。局所 limit/allocation 拒否は `ResumeLocalLimit`（peer の不正としない）。
  - `open_sink(boundary_bytes)` で書込位置を境界に合わせる。再開後に完成した spill は中断なしの spill とバイト同一（試験で確認）。
- 順序: spill の collection 順をそのまま継続する。canonical な collection 順序は RFC37 に未定義（codec 表の領域）で、本差分は創作しない。
- ファイル: `consensus/core/src/palw_row_stream_v1/resume.rs`（新規 leaf）。

試験: `consensus/core/tests/ibd_row_stream_resume.rs`（新規、8 件）。下記 §3。

## 3. 試験証拠
同一 source（本 branch の code commit）、既定 feature、`CARGO_INCREMENTAL=0 ~/Downloads/MISAKA-wt-b/buildslot.sh cargo test --offline -p kaspa-consensus-core …`。
- `--test ibd_row_stream_resume --test ibd_row_stream_generations`: exit 0。`ibd_row_stream_generations` 9 passed、新規 `ibd_row_stream_resume` 8 passed、0 failed、0 ignored。新規 8 件は torn tail の切捨てと再開後 spill のバイト同一、全 cut 位置（0..=全長）の安全性、完了済み collection の 1 byte 改変の拒否、別世代（preimage/collection）の拒否、重複 collection の拒否、byte 予算の再計上と持越し、局所 limit の分離、coverage の欠落/過剰/位置違い/重複 layout 拒否。
- `--lib -- palw_row_stream_v1`: exit 0、6 passed、0 failed、0 ignored、5210 filtered out（既存 spill_reader 試験の回帰確認）。
- ログ: `/private/tmp/claude-501/lanef-a.log`、`lanef-b.log`。full suite・clippy・一括 fmt は未実行。

実 node campaign ではない（fixture 上の PASS）。15 gate は更新しない。wire を使わない node-local verifier のみで、P2P 受入の代用ではない。

## 4. 公開 API と所有権
- `PalwRowStreamVerifierV1::finish_covering`、`palw_resume_row_stream_v1`、`PalwRowStreamResumedV1`、error 変種 6 つ（`PalwRowStreamErrorV1` は `Clone + PartialEq`、網羅 match の外部 caller なし）。
- 所有: sink（spill file）は caller。resume は sink を境界位置で開く closure を受け取り、replay 中に spill を書き換えない。失敗時は verifier を返さない（昇格なし）。
- wire tag・既存 ID・DB schema・consensus root の意味は不変。互換性変更なし。

## 5. 残件
- CODE: (a) collection codec 表と serve 側（label → key/value 型、projection、非標準 root、fence 依存 block）。表が出来たら `finish_covering` の required を表から導出する。(b) spill → typed carriage の組立と `restore_native_carriage_v1` への接続、NativeDemand 予約。(c) typed material を P2P で serve/request する経路。
- TEST: 上記が揃った後の「serve → wire → assembly → import → cold reopen」を同じ候補で通す試験、切断 resume の実 TCP 試験、サイズ超過。
- EXTERNAL: なし。
- POLICY: RFC37 採択と protocol version 割当、typed material の wire 採択、material の実 price/payer/TTL（Lane C の所有。本差分は保持 policy を作っていない）。

## 6. 閉じた要件 / 閉じない RFC
- 閉じた: RFC37 §2.1 の「stream が名指しした集合の完全性」を typed に拒否する部品、§5.3 の resume 部品（node-local）。
- 閉じない: RFC37 全体（wire・codec 表・typed 組立・NativeDemand 未接続）、RFC32、RFC08 の provider campaign、RFC02 import 資源境界の row stream 経路。
