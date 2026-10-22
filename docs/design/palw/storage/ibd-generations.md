# IBD・reorg・移行 — 有界ストリーム import と世代番号付き保存(G14 storage / IBD lane)

Status: **設計メモ + node-local 実装**(2026-10-22, branch `storage/ibd`)。P2P に見える row stream の wire は
[RFC-0037](../../../rfc/0037-ibd-bounded-row-stream-and-storage-generations.md) /
[ADR-0204](../../../adr/0204-ibd-bounded-row-stream-and-storage-generations.md)(**Proposed / Review Required**)で、
P2P message は**未登録・休眠**。consensus(state root の形式、header、fork choice、抽選)は一切変えていない。

前提: pruned node の総 footprint ≤ 30,000,000,000 bytes(G14 storage 共通方針)。本メモの対象は「IBD の受信・展開・
DB 切替・再起動・reorg の間もその上限内に留まる」ための IBD 側の仕組み。

## 1. 現状(コードを読んだ結果)

### 1.1 pruned IBD の流れ(`protocol/flows/src/ibd/flow.rs`)

`IbdType::DownloadHeadersProof`:

1. `consensus_manager.new_staging_consensus()` — 新しい `consensus-NNN/` を作る(`consensus/src/consensus/factory.rs`)。
2. `ibd_with_headers_proof(staging)` — pruning proof → headers → trusted anticone → **PALW carriage を staging に import**
   (`request_pruning_point_palw_state` + `import_pruning_point_palw_state`)→ `validate_staging_palw_order`。
3. `authorize_commit` → `staging.commit_if(...)`(`components/consensusmanager/src/lib.rs`)— in-memory の差し替え、
   `ctl.make_active()` = `MultiConsensusManagementStore::commit_staging_consensus()`、**直後に旧 consensus ディレクトリを削除**
   (`delete_inactive_consensus_entries`)。
4. **新 active 上で** `sync_new_utxo_set`: PALW carriage を**もう一度**取得・import、overlay、UTXO を clear して chunk stream
   (MuHash は最後に照合)、EVM snapshot、`complete_pruning_point_import_for_session`。成功で `is_utxo_stable = true`。
5. block bodies。

`IbdType::Sync` で `!is_utxo_stable` のときは 4 だけを現 active 上で再実行する。

### 1.2 世代(consensus DB)の切替

- 世代 = `datadir/consensus/consensus-NNN/`。どれが current/staging かは管理 DB(`meta`)の `MultiConsensusMetadata`
  (`current_consensus_key` / `staging_consensus_key`)だけが知っている。
- 起動時(`Factory::new`)に staging と非 current を全削除(archival は staging のみ)。
- 「検証済み」の区別は consensus DB 内の `is_utxo_stable` と `PruningImportSessionStateV1` の epoch。

### 1.3 PALW carriage の取得と import

- 転送: protocol 106 chunk / 107 zlib(`protocol/flows/src/palw_snapshot_chunks.rs`、`ibd/palw_snapshot_chunks.rs`)。
  `SnapshotSpill` は `std::env::temp_dir()` に作って即 unlink(fd のみ)、node 全体で同時 2 session(semaphore)、
  encoded 上限 `PALW_PRUNING_STATE_MAX_BYTES` = 384 MiB、decoded 上限 512 MiB。
- chunk hash と stream root は**送信者が選んだ**転送完全性だけ。consensus の root との照合は全量を受け取って
  **メモリに全 decode した後**。
- import(`VirtualStateProcessor::import_pruning_point_palw_snapshot`): witness = pruning point の
  **selected-chain 上の子** header の `palw_state_root`(peer の言い分ではない)→ frontier ≤ point の blue score →
  `restore_native_carriage_v1`(= `into_state_v3`: carriage 形状検査、index 再構築、`assert_internal_consistency*`、
  root 照合)→ 1 つの `WriteBatch` で tip 行 + pruning snapshot 行 + class 宣言 + `is_utxo_stable=false` →
  `write_pruning_import_barrier`。
- UTXO は chunk ごとに DB へ直接書く。MuHash 不一致なら DB に部分行が残るが stable flag が false なので次回 clear される
  (「部分 state を昇格しない」は flag で担保)。
- EVM snapshot ≤ 256 MiB、overlay ≤ 64 MiB はそれぞれ単一 message をメモリで受ける(stream ではない)。

### 1.4 ピーク disk 使用量(現状)

| 局面 | 同時に存在するもの | 備考 |
| --- | --- | --- |
| staging 構築中 | **active DB(全量)+ staging DB(proof・headers・trusted・PALW)** | 2 DB。UTXO と bodies は commit 後に新 active へ入るので、staging 側は「全量」ではない |
| commit の瞬間 | 両 DB → 旧 DB 削除 | 削除は commit 直後、同期的 |
| PALW install | carriage × 2 行(tip + pruning snapshot)+ WAL | 一時的に carriage の約 4 倍(現 t12 ≈ 57 MB、上限 384 MiB なら最大 ≈ 1.5 GB) |
| PALW 受信 | spill 384 MiB × 2 session | **temp_dir = 管理領域の外**(capacity lane が管理領域へ移す) |
| UTXO 再 import | clear の tombstone + 新行 + compaction 一時 | WAL/compaction 予算側 |

実測(live t12 node1, DAA ≈ 13.7k): consensus 7.5 GB。staging の実測は未取得(drill 後の宿題、§6)。

### 1.5 見つかった問題(本 lane の扱い)

| # | 問題 | 扱い |
| --- | --- | --- |
| F1 | 切替 metadata の書き込みが fsync されない(RocksDB 既定 `sync=false`)。process kill には耐えるが OS crash / 電源断には耐えない。commit 直後に旧ディレクトリを消すので、OS crash で metadata が旧 key に戻ると**消えたディレクトリを current と呼ぶ**(genesis なしの空 consensus)。 | **修正**: `commit_staging_consensus` / `new_staging_consensus_entry` / `save_new_active_consensus` の後に `flush_wal(true)` |
| F2 | cancel 時に staging ディレクトリの削除に失敗すると、次の staging が作られても再起動まで残る(3 DB)。 | **修正**: `new_staging_consensus` の先頭で非 active 世代を削除してから予約・作成 |
| F3 | PALW spill が `temp_dir`(管理領域の外、tmpfs のことも)。 | **capacity lane が実装中**(`managed_temp_dir()`、`--storage-cap-bytes` 時のみ)。本 lane は重複実装を取り下げた |
| F4 | staging DB・spill の前に容量予約がない(並行タスクが同じ空きを数える)。 | **capacity lane が実装中**(`StorageBudget::reserve_after_reclaim(StagingDb / IbdSpill)`)。本 lane の F2 cleanup はその予約の直前に来る |
| F5 | commit で旧世代を即削除するため、切替をまたぐ巻き戻しができない。commit 後・UTXO import 完了前の crash は `is_utxo_stable=false` で「未検証」と判別できるが、戻る先はない(再 sync)。 | 設計(§3.4)。IBD の意味を変えるため本 lane では未変更 |
| F6 | proof 経路で PALW carriage を 2 回取得(staging と commit 後)。disk ピークではなく時間と帯域。 | 記録のみ(consensus 側 flow の変更になるため別途) |
| F7 | 受信 carriage は全量メモリ decode。row 単位の検証・spill はない。 | §3.2 の verifier を実装(wire は休眠) |

## 2. 予算(設計値、bytes)

| 項目 | 予算 | 本 lane の上限の作り方 |
| --- | ---: | --- |
| active DB | 10,497,346,816 | capacity lane |
| staging DB | 10,497,346,816 | 作成前に予約(capacity lane の `StagingDb`)。本 lane は予約前に負けた世代を削除(F2) |
| IBD spill | 805,306,368 (= 384 MiB × 2) | `SpillArea`(1 file ≤ cap、同時 ≤ 2、各 file の cap を作成前に予約)/ 既存 `SnapshotSpill` は capacity lane が session ごとに 384 MiB を予約 |
| WAL/compaction | 4,000,000,000 | capacity lane。PALW install の ~4× 一時はここに入る |

「設定値の合計は保証ではない」— 本 lane が保証するのは「世代は同時に最大 2」「spill は file ごとに cap 超過で失敗」
「予約は作成の前」の 3 点で、実使用量の測定は capacity lane。

## 3. 目標設計

### 3.1 世代番号付き保存と原子的 switch journal(`kaspa_database::storage_generations`)

```text
root/
  GENERATIONS          journal(唯一の真実): seq, next, active, previous, staging + checksum
  GENERATIONS.new      書きかけ(起動時に破棄)
  gen-000007/          active
    VERIFIED           検証済み marker(gen 番号・subject・root・whole_state_checked=1 + checksum)
    undo.log           基底より上に適用した block の undo
  gen-000006/          previous(切替をまたぐ reorg 用)か staging(構築中)— どちらか一方だけ
```

- journal 変更は常に「`.new` を書く → fsync → rename → ディレクトリ fsync」。
- **世代は同時に最大 2**: active +(previous または staging)。`begin_staging` は「古い staging・previous を退役(削除)→
  予約 → journal に staging を記録 → ディレクトリ作成」の順。負けた世代は**新しい空間を予約する前に**消える。
- `VERIFIED` は whole-state check を経た証拠(§3.2)からしか作れない。`whole_state_checked=false` の marker は
  `RootAloneIsNotVerified` で拒否。journal が `active` に指せるのは検証済み世代だけ。
- 起動時の復旧(`GenerationStore::open`):

| crash した位置 | 復旧後 |
| --- | --- |
| 退役の直後 / 予約前 | 旧 active(previous は消えている) |
| staging の `.new` 書込後・rename 前 | 旧 active。`.new` 破棄 |
| staging rename 後 / ディレクトリ作成後 / 充填中 | 旧 active。staging は**再開しない**(切替判断は durable でない)→ 削除 |
| `VERIFIED.new` 後 / `VERIFIED` 後(未切替) | 旧 active。staging 削除 |
| switch の `.new` 後・rename 前 | 旧 active |
| switch の rename 後 | **新 active**、previous = 旧 |
| 退役・巻き戻しでディレクトリ削除後・journal 前 | journal が指す方。参照されないディレクトリは削除 |
| active の marker が壊れた(disk 故障) | 検証済み previous へ fallback。無ければ `active=None` = 再 sync(未検証 state は渡さない) |
| journal 自体が壊れた | `JournalCorrupt` で停止し、検証済み候補を列挙(推測しない) |

### 3.2 有界な authenticated stream import

state root は今も「各 collection の 64 byte root を並べた preimage の hash」(ADR-0043 §2、
`palw_state_proof_v1.rs`)。**root の形式を変えずに**次ができる(`consensus/core/src/palw_row_stream_v1.rs`):

1. 期待 root は witness header(pruning point の selected-chain 子)から取る。peer の preimage を最初に
   `palw_state_root_of_preimage_v1(preimage) == expected` で照合 — **row を 1 つも受け取る前に**。
2. collection ごとに `(label, 行数, preimage 内 offset)` を宣言させ、row を 1 つずつ `PalwCollectionRootStreamV1`
   (`palw_collection_root_of_entries_v1` とバイト同一)に流しつつ capped spill に書く。
3. collection の終わりで root を preimage[offset..offset+64] と照合。**不一致はその collection で停止**(以降は読まない)。
   verifier は poison され、spill は drop で消え、staging 世代は marker なしで破棄される。
4. offset は範囲内・重複なし、label 重複なし、宣言行数の過不足は拒否。byte 予算超過・spill cap 超過は local な中断。
5. root が projection にかかる collection(`gen_classes` / `tir_classes` の `rooted_bytes_v1`)は全 record を運び、
   `begin_projected_collection` で projection を hash する。非標準 root(`kernel-aux-v32` など)は同じ byte 予算内で
   buffer し、light client と同じ `PalwCollectionOpeningV1::root` で照合する。

**ここまでは「verified stream」で、証拠ではない。** 全 collection を名指ししたか(preimage の slot 配置は fence と
存在する表に依存し、受信側は state なしに列挙できない)と、今の import が行う全体整合性検査は、必須の第 2 段
`PalwRowStreamVerifiedV1::whole_state_checked` で行う。第 2 段の実体は**既存のゲートそのもの**:

- spill から typed carriage を組み立て、`restore_native_carriage_v1(expected root)` を呼ぶ。これは
  `into_state_v3` の carriage 検査・index 再構築・`assert_internal_consistency*`・**全 root の再計算**を行うので、
  名指しされなかった collection は root 不一致で落ちる。または
- 検証済み基底からの replay。

第 2 段の `Ok` だけが `PalwWholeStateVerifiedV1` を生み、世代の `VERIFIED` はそれからしか作らない。
**root を受け取っただけ、あるいは root に一致する stream を受け取っただけでは検証済みにならない。**

等価性の論拠: 最終判定は現行と同一関数で、stream は「早期中断」と「disk の上限」を足すだけ。より弱い検査に
置き換えた部分はない。phase 1 ではメモリ使用は現行同様 O(state)。

phase 2(state lane の authenticated map が前提): 全体検査をメモリ上の `PalwChainStateV2` から、世代 DB 上の
index/range 検査へ移す。`into_state_v3` の検査を分類すると:

| 検査 | 種類 | phase 2 の形 |
| --- | --- | --- |
| class/claim semantics の行内整形、IR/gen class の program hash | row-local | stream 中に行ごと |
| claim semantics → claims/liabilities/vesting/class の存在 | cross-table lookup | 世代 DB の point lookup |
| improvement 表の整合 | cross-table range | 世代 DB の range |
| `safe_weight` の再導出、share 表の key 集合、deadline/vesting/capacity index | **global aggregate** | 世代 DB 上の 1 pass 集計(bounded memory)— 同値性テストを先に作る |

### 3.3 reorg 用 undo

- 世代 = 検証済み基底 + その上に適用した block。block ごとに旧値 image を `undo.log` に追記(長さ・checksum・seq、
  fsync)。torn tail は起動時に切り詰め、seq の欠落はそこで信頼を打ち切る。旧値 image の復元は冪等なので、
  「復元した後・pop する前」の crash は同じ undo を再実行するだけ。
- 基底より深い reorg → 保持している検証済み previous に `roll_back_to_previous`(journal で原子的)し、そこで undo。
- previous を退役した後の深い reorg は `NoPrevious` = **再 sync(停止して取得)**。block の無効判定には絶対にしない
  (「取得できない」≠「無効」)。consensus 上 pruning point より下の reorg は元々ないので、previous が要るのは
  「切替直後、新基底がまだ final でない間」だけ。

### 3.4 consensus-NNN への適用(phase B、未実施)

1. `MultiConsensusMetadata` の current/staging を `GenerationStore` の journal に置き換えるか、journal を併記して
   起動時に照合する(本 lane では F1 の fsync で同等の durability だけ先に入れた)。
2. 旧世代の削除を commit 直後から「新世代の pruning import 完了(`is_utxo_stable`)後」へ遅らせ、その間 previous として
   保持する。DB 数は 2 のまま(staging 構築中と同じ)なので予算内。commit 後・UTXO import 前の crash で戻る先ができる。
   ただし IBD の意味(負けた chain に戻れる期間)を変えるので review 対象。
3. PALW の 2 回取得(F6)は、staging で import した carriage を commit 後に再利用すれば 1 回にできる。

### 3.5 容量 lane とのインタフェース

作業開始時、`storage/capacity` には予約 API が無かったので最小の trait を定義した。作業中に capacity lane の
worktree(未 commit)に `kaspa_utils::storage_budget::{StorageBudget, StorageCategory, StorageReservation}` が現れ、
staging DB と PALW spill の予約・管理領域配置はそちらが実装していた。そのため本 lane は同じ箇所への重複実装
(Factory の staging 予約、`SnapshotSpill` の配置・予約)を**取り下げ**、generation store / `SpillArea` が使う
trait だけを残した。trait は RAII guard を返す 1 メソッドにしてあり、統合時の adapter は数行
(`database/src/storage_generations/reserve.rs` の doc に雛形):

```rust
pub trait StorageReserver: Send + Sync {
    fn reserve(&self, purpose: StoragePurpose, bytes: u64) -> Result<ReservationGuard, ReserveRefused>;
}
// StagingGeneration → StorageCategory::StagingDb, IbdSpill → StorageCategory::IbdSpill
```

拒否は常に local な容量不足(IBD は停止・再試行)で、peer や block の判定に変換しない。

## 4. 実装したもの

| ファイル | 内容 |
| --- | --- |
| `database/src/storage_generations/mod.rs` | `GenerationStore`(journal・VERIFIED marker・復旧・退役・切替・巻き戻し・crash point) |
| `database/src/storage_generations/reserve.rs` | `StorageReserver`(RAII guard を返す)/ `StorageReservation` / `UnboundedReserver`(既定)/ `FixedBudgetReserver` |
| `database/src/storage_generations/spill.rs` | `SpillArea` / `SpillFile`(cap 超過は `StorageFull` で失敗、作成前予約、drop で削除、起動時 sweep、high-water) |
| `database/src/storage_generations/undo.rs` | `UndoLog`(fsync 追記、torn tail 切詰め、pop) |
| `consensus/core/src/palw_row_stream_v1.rs` | `PalwCollectionRootStreamV1`、`PalwRowStreamVerifierV1`、2 段の証拠型、spill 読み戻し |
| `consensus/src/consensus/factory.rs` | F1 切替 metadata の fsync、F2 staging 作成前に非 active 世代を削除 |

テスト:

- `kaspa-database` の `storage_generations::tests`: 全 crash point → 復旧が検証済み世代を選ぶ、退役・巻き戻し中の crash、
  未検証世代は active にならない、marker 破損時の fallback / 再 sync、journal 破損の拒否、「負けた世代を予約前に削除」
  (2 世代ぶんの予算で 3 回連続切替が通る)、切替をまたぐ reorg(gen 2 の基底より深い fork → gen 1 へ巻き戻し → undo →
  fork 適用、退役後は `NoPrevious`)、undo log の torn tail、spill の cap / 予約 / sweep。
- `consensus/core/tests/ibd_row_stream_generations.rs`: streamed root = 既存 root(バイト同一)、本物の genesis preimage、
  正常系の昇格と spill の peak ≤ 2 × cap、**stream 途中の不一致で何も昇格しない**(2 番目の collection で停止)、poison、
  cap 超過の clean abort、root 一致でも whole-state で拒否、検証〜切替間の crash、projection collection。

## 5. やっていないこと・条件

- P2P message(RFC-0037)は未登録。serve 側(preimage と全 collection の row 生成、collection ごとの codec 表)も未実装。
- 受信 row → typed carriage の組み立て(第 2 段の PALW 実体)は未実装。テストでは第 2 段を toy の全体不変条件で代替した。
- consensus-NNN を `GenerationStore` に載せる phase B(§3.4)、旧世代削除の遅延、PALW 2 回取得の解消は未実施。
- staging DB・PALW spill の予約と管理領域配置は capacity lane の実装に委ねた(重複を避けるため本 lane では取り下げ)。
- `StorageReserver` → `StorageBudget` の adapter は capacity lane の merge 後に追加する。
- staging DB の実測、30 GB 超 canonical state での IBD drill は未実施(共通方針どおり drill は後)。

## 6. 次の手順

1. capacity lane の merge 後、`StorageReserver` の adapter を足し、generation store / `SpillArea` を
   `StorageBudget` に接続。factory.rs は両 lane が同じ関数を触るので、F2 cleanup → capacity の予約 の順で統合する。
2. RFC-0037 / ADR-0204 の review。採択されたら serve 側と組み立て段を実装し、新 protocol version の下で
   既存 106/107 と並存(旧 peer は従来の carriage 転送のまま)。
3. phase B(§3.4)を別 RFC/ADR なしで入れられるか(node-local か)を review で判断。
