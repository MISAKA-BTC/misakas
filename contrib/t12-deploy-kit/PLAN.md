# testnet-12 再 genesis（R-core+）— 4 ホスト配備計画

作成: 2026-09-23（5a559459 時点）。**2026-09-25 に R-core+（ADR-0152 v3.1）向けに更新**: 基準は統合線 `rcore/int-3`
@ `a0af3c92`、この kit は branch `rcore/release-prep`。**この kit はまだ何も実行していない。リモートには一切触れていない**
（この更新で実行したのは Mac 上の `bash -n`・launch script の生成と `--check`・`probe-identity-local.sh` だけ）。
リリース固有値（binary sha256・fingerprint・genesis・premine txid）は **出荷する commit からだけ** 取る。`fleet.env`
（`fleet.env.example` の写し。`*.env` は gitignore）では `__FILL_ME__` で、これが残っている限り `stage` / `switch` は動かない。
公開までの順序・gate の証拠・再 pin の手順は `docs/t12-rcore-launch-checklist.md`（この kit と同じ commit）。

---

## 0. 公開を止めているもの（先に読む）

| # | 何か | 誰が |
|---|---|---|
| **R1** | **出荷する commit がまだ無い**。統合線 a0af3c92 に、監査の deadline＋P-1（4-quater: `palw_class_verify_deadline`、pruning depth ≈ 74,920 DAA、2M を規則で閉じる）、A-held（`feat/t12-aheld` ＋ `feat/t12-aheld-node`: object 57 の自動応答・N4）、Activation Pool（`feat/t12-activation-pool`）、P2-8 の自動 filer（`rcore/p2-file`）が未 merge。どれも t12 の params を動かし、genesis を動かし得る。→ `EXPECT_FP` / `EXPECT_GENESIS` / `PREMINE_TXID` は出荷 commit の probe で固定する（§4）。a0af3c92 での値は **暫定**（checklist §3） | 実装・監査 → ユーザー |
| **R2** | ADR §8.3 の launch gate（item 1〜9、IA-14 の ship 条件）が出荷 commit で未充足。項目と証拠は checklist §1〜§2 | 実装・監査 |
| R3 | `HB_ADDR`（heartbeat の支払先）: 旧 kit の `misakatest:qf6hf5v0…` は **card 0 の payout address** だった（`t12_deploy_kit_constants` が `PALW_T12_GENESIS_BONDS[0].payout_payload` から全桁を出す）。`fleet.env.example` に全桁を入れた。運用者が確認するか別の address にする。§10 Q10 | **運用者**（確認） |
| R4 | 公開後の drill ホストが無い。公開後は ibm・.113・5.104 のすべてが公開 t12 node を動かすので、drill kit はこの 3 台を拒否する（ADR §8.3 item 3「公開 node のあるホストで drill しない」）。95.111 は seeder ホスト（11 GiB、egress 制限）で、rcore drill script の host guard も拒否する | **運用者**（公開後の drill 用に 4 台以上を用意） |
| B3 | 5.104 で別 session の node が動いていれば `install-5104.sh switch` は拒否する（旧 live の floor seat・private chain・scan・daa-obs） | 別 session が停止 |
| B5 | ibm の `misaka-t11-node0` が enabled。`DISABLE_T11_NODE0=1` で switch（§10 Q5 決定済み） | ユーザー |
| B6 | misakascan は現在 private chain / 旧 chain を表示。新 chain＋新 DB への配線は `../misakascan-t12/DEPLOY.md` §9 の **どちらか一方**（推奨 `deploy.sh`、`install-113.sh explorer-apply` は使わない。両者は互いの drop-in があれば拒否する）。決定は運用者 | ユーザー |

旧 B1（genesis 衝突）は 09-24 に (a) で決定済み（§10）: t12 の premine txid は t12 固有の `5e0d5f1b…`、genesis は a0af3c92 で
`a27f8f44…`。f6cc9576… 以下の旧・私設・drill の genesis はすべて `FORBIDDEN_GENESIS` に全桁で入れてあり、install は拒否する。
旧 B2（DoS 修正未 merge）は統合線に入った。旧 B4（8k artifact）は 09-24 に解消済み（§10）。

## 1. 配置と役割

8 bond 全部が 8k（class `ebf44d0a…`、Qwen2.5-1.5B graph-v7@8192）の panel seat。class の lifecycle は ready 7 seat が要るので余裕は 1。
2M class（`74c67e63…`）は **公開時点で閉じている**（ADR §8.3 item 7 の IA-12 修正、U-D1、O-11）: どの node も 2M の artifact を持たず、
2M の producer にもならない。kit は 2M の class を producer / artifact に指定した node を stage の時点で拒否する（`lib.sh` `build_args`）。

| host | bond | unit | 種別 | 役割 | heartbeat | share（§2） |
|---|---|---|---|---|---|---|
| ibm 169.58.39.220 | 0 | `misaka-t12-node0` | **新規 unit** | 8k producer（fleet で唯一）+ 8k seat + panel + round lane | ✔ | 10,496 MiB / MemoryMax 20G |
| ibm | 1 | `misaka-t12-node1` | drop-in | floor producer + 8k seat + panel + round lane | | 8,192 MiB / 16G |
| .113 169.58.232.113 | 6 | `misaka-t12-node` | drop-in | floor producer + 8k seat + panel + round lane + explorer backend | ✔ | 8,192 MiB / 17G |
| 5.104.81.23 | 2 | `misaka-t12-seat2` | drop-in | 8k seat + panel + round lane（**8k producer はやめた** — §2.3） | | 3,584 MiB / 9G |
| 5.104 | 3 | `misaka-t12-seat3` | drop-in | 8k seat + panel + round lane | | 3,584 MiB / 9G |
| 5.104 | 4 | `misaka-t12-seat4` | drop-in | 同上 | | 3,584 MiB / 9G |
| 5.104 | 5 | `misaka-t12-seat5` | drop-in | 同上 | | 3,584 MiB / 9G |
| 5.104 | 7 | `misaka-t12-seat7` | **新規 unit** | 同上（card 7 は 09-23 に 5.104 で再鍵、鍵はこの host にしか無い） | | 3,584 MiB / 9G |
| 95.111.236.186 | — | `misaka-dnsseeder-t12` のみ | 変更なし | seeder3（seeder は 4 台とも変更不要 — `SEEDERS.md`） | | — |

- producer: floor 2 本（ibm b1・.113 b6）、8k 1 本（ibm b0）。5.104 b2 は 09-24 の計画では 8k producer だったが、release-prep review
  の指摘で seat のみにした（§2.3。自分の DA 応答が attempt と seat の空きを待つ構造だったため）。全 node が round lane を動かす
  （09-25 から flag ではなく **duty**: bond を持つ node では常に動く。§14）。
- share と MemoryMax は「share / MemoryMax」。MemoryMax は crash guard で host の分割ではない（§2.1）。
- fee float は `$PREMINE_TXID:(41+N)`（card 7 → 48）、bond は `$PREMINE_TXID:N`。producer の支払先は各 bond 鍵自身のアドレス（既定、§10 Q10）。
- **R-core+ が node に足した仕事（どれも flag は無い。bond（`--palw-producer-key` ＋ `--palw-producer-bond`）と `--palw-fee-outpoint` を持つ node で
  自動で動き、§2 の ledger に予約を取る。09-25 から panel 自体も flag ではなく duty — §14）**:
  - SEAT-R: seat は replay した場合だけ Valid に署名する（whole-job の full seat、または S1 の partial segment）。
  - P2-7: DA session への応答（producer は自分の claim、Valid signer は lock が覆う claim を `MaterialDisclosedV2` で答える）と、
    R-core の court がまだ求め得る capture の保持（retention janitor。disk の話で memory ではない）。
  - P2-9: heartbeat block が lifecycle carrier（有罪・reveal）を運ぶ。heartbeat miner の既定動作。
  - P6 / U2: producer は ledger と producer floor（13,000 MSK）を見てから掘る。genesis bond は ~939k MSK なので通る。
  - P2-8 の自動 filer（DefaultAccused / PanelFalseValidV2 / ExecutorRefuted）は `rcore/p2-file` にあり **a0af3c92 に未 merge**。
    merge 後も flag は無い（fee outpoint を持つ node で動く）。その replay 用 ledger 予約は §2。
- 公開 node に drill の flag は絶対に載らない（ADR §8.2）。`--palw-drill-genesis-salt` と他の `--palw-drill-*` は launch script が
  exit 78 で拒否し、`KASPAD_*` 環境変数も拒否し、drop-in は継承した `Environment=` / `EnvironmentFile=` を空にする。drill の marker が
  ある appdir も拒否する。`switch` は "PALW DRILL" を名乗る node を止め、同じホストで drill が動いていれば切替自体を拒否する。
- `--enable-unsynced-mining` は使わない。heartbeat は peer さえあれば古い genesis timestamp でも打つ（`palw_heartbeat_miner.rs`、`should_mine` を通らない）ので、b0 が b1 を見た瞬間に時計が進み、producer の sync 判定も通る。
- 5.104 は ufw で INPUT DROP、全 node が 127.0.0.1 listen で .113/ibm に外向き接続（従来どおり）。

## 2. メモリ予算（R-core+ の resource profile から再導出、2026-09-25。release-prep review で ledger の実際の式に合わせて改訂）

### 2.1 規則 — ledger が仕事を始める条件（`kaspad/src/palw_memory_ledger.rs`、`palw_backends.rs`）

node は仕事ごとに「holding（artifact file の bytes）＋ role の resource profile の working set」を 1 つの ledger に予約し、
**次の式を満たすときだけ** 始める。足りなければ毎 tick 再試行する（ブロックは拒否しない。preempt は無い）。

```
need ≤ min( share , min( MemAvailable , memory.max − memory.current ) − 1 GiB ) − reserved
        ^^^^^ --palw-host-memory-share   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ host_available_bytes_v1（cgroup v2 は unit と祖先の最小）
```

- `share` を宣言すると live 項は 70 % の haircut でなく `past_reserve`（= 上の `− 1 GiB`）になる（`palw_memory_ledger.rs` `bounds`）。
- **`memory.current` は page cache を含む**（cgroup v2）: artifact を最初に fault した node の cgroup に 8k file の 1,716 MiB が課金され、
  RocksDB・log・retention の capture の page も溜まる。**走っている仕事の working set は `reserved` と `memory.current` の両方に数えられる**。
  だから unit に `MemoryMax` を付けると、宣言した share に届く前に ledger が拒否し得る（旧 §2 はこの項を落としていた — review の high）。
- `MemAvailable` は再利用可能な page cache を「空き」に数えるので、host 項は物理メモリの守りとして正しく働く。
  **host を守っているのは ledger の MemAvailable 項で、`MemoryMax` ではない。** `MemoryMax` は node ごとの crash guard（ledger の外で
  伸びるもの — IBD の cache、mempool、RPC — の暴走を、その node だけの OOM にする）であり、host の分割ではない。unit の MemoryMax の和は
  MemTotal を超えてよい。
- `--ram-scale` は share から導かれる（0.0375/GiB、`palw_ram_scale_for_share_v1`）。consensus cache の宣言は ram-scale 1.0 で 1.34 GiB。

### 2.2 一つの仕事の予約額

a0af3c92 の値。`kaspad/tests/t12_role_memory_figures.rs`（`#[ignore]` の worksheet）がこの build で再計算して印字する:

```bash
CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 cargo test --locked -p kaspad --test t12_role_memory_figures -- --ignored --nocapture
```

条件は t12 genesis の 8k 行・canonical (1023, 2)・A16-KV-i16（出荷値 `KV_STORAGE_SHIPPED_V1`）・8 thread・prefill run 64。
8k artifact は 1,799,359,436 B = 1,716.003 MiB（committed sidecar の `artifact_bytes`。worksheet は MiB を切り上げるので 1,717。
以下の数字は worksheet の印字どおり）。

| 仕事（ledger の role 名） | 導出 | need（MiB） | W = 触る working set（MiB） |
|---|---|---|---|
| 8k producer の attempt（`producer`） | 1,717 + working set | **3,456** | 1,740 |
| 8k full seat の replay（SEAT-R `full-seat`）、court、DA の応答（P2-7 `da-answer`。自分の claim も covering signer も full seat の額） | 1,717 + working set | **3,456** | 1,740 |
| 8k partial seat の segment（S1 `partial-seat`、5 seat の最悪 segment 3。capture は streamed fold ＋ edges ＋ kept list に置換 — `palw_partial_seat_streamed_need_v1`） | 1,717 + working set | **3,500** | 1,784 |
| floor（BASE-0）の attempt / replay / DA 応答 | BASE-0 は profile を持たない → holding（8k を持つ node では 8k file）＋ 512 MiB の推定 | **2,229** | 512 |
| P2-8b の replay filer（未 merge） | 8k の canonical capture は 105,518,224 leaves でホストの上限 2^26 を超えるので **8k では走らない**（streamed routes の仕事）。floor では小さい | floor で ≤ 2,229 | |
| 2M（閉じている） | 参考: 2.67 GiB の artifact ＋ producer / full seat の working set 9,980 MiB | 使わない | |

seat の replay は同時に 2 本まで（`PalwSeatReplaysV1::IN_FLIGHT = 2`、`palw_panel.rs`）。DA の応答は拒否されると次の tick に再試行し、
予約を先取りしない。seat の readiness proof は full seat の額（3,456）の `can_reserve` が通るときだけ出る（`palw_panel.rs`
`replay_memory_budget_v1`）。

### 2.3 node ごとの share と MemoryMax

**share** = 同時に走らせたい仕事の和 ＋ 端数（preflight は Σshare ＋ RESERVE ≤ MemTotal を確かめる — host の分割はこちら）。
**MemoryMax** は §2.1 の cgroup 項が share を下回らない値:

```
MemoryMax ≥ share + base + artifact 1,716 + ΣW(ほかに走っている仕事の最悪) + 1,024 + cache 余裕 2,048
```

- `base` = PALW の仕事が無いときの process（ram-scale が宣言する consensus cache ＋ 256 MiB。.113 b6 は gRPC・utxoindex・EVM で ＋512）。
  **推定値**。switch 後に `install-<host>.sh check` が出す process RSS と cgroup の anon で置き換える。
- `ΣW` = share の範囲で「もう 1 本入る」状態の、走っている仕事の working set の最大（worksheet が列挙する）。
- `cache 余裕` = RocksDB・log・retention の capture の page cache（held 8k の attempt capture は fold で数 MiB）。**上限ではない**。
  cache がこれを超えて溜まると cgroup 項が share を下回る。`check` は unit ごとに `memory.max − memory.current − 1 GiB` を share と比べて警告し、
  `t12check` は `getPalwNodeStatus` の live（`memoryHeadroomBytes`）が share を下回ると警告する。その node の memmax を上げるか、`-`
  （MemoryMax=infinity）にする（kit の node 表の 1 欄。§10 Q4 の「MemoryMax あり」の再確認 **【確認】**）。
- stage は `MemoryMax ≥ share + artifact + 1 GiB` を満たさない行を拒否する（`lib.sh` `check_memmax`）。

| node | 同時に走らせる仕事 | 和 | share | base（推定） | ΣW | MemoryMax の下限 | MemoryMax |
|---|---|---|---|---|---|---|---|
| ibm b0（8k producer + seat） | attempt 3,456 ＋ 自分の claim の DA 応答 3,456 ＋ seat 3,500 | 10,412 | **10,496** | 784 | 3,566（seat 2 本） | 19,635 | **20G** |
| ibm b1（floor producer + 8k seat） | floor attempt 2,229 ＋ 自分の floor claim の DA 応答 2,229 ＋ seat 3,500 | 7,958 | **8,192** | 668 | 2,295（seat ＋ floor） | 15,944 | **16G** |
| .113 b6（同上 ＋ explorer backend） | 同上 | 7,958 | **8,192** | 1,180 | 2,295 | 16,456 | **17G** |
| 5.104 b2 b3 b4 b5 b7（seat のみ） | seat の仕事 1 本 3,500（2 本目の replay と covering signer の DA 応答は待つ） | 3,500 | **3,584** | 437 | 0 | 8,810 | **9G** |

旧 kit の MemoryMax（b0 14G・b1/b6 11G・b2 10G・seat 6G）では、5.104 の seat は `memory.current` が約 1.6 GiB を超えると（artifact の
page cache 1,717 MiB が課金されただけで）8k の replay も readiness proof も止まり、8 seat 中 5 seat のある host で 8k の ready 7 を
割り得た。ibm b0 は attempt と seat が走ると自分の DA 応答が入らなかった（review の算術、`memory.current` ≈ base ＋ 1,716 ＋ ΣW）。

**5.104 b2 は seat のみ（既定、review の指摘で option (b) を採用）**。8k producer は ibm b0 の 1 本。b2 を 8k producer に戻すには
share を 6,956 以上（attempt ＋ seat）、MemoryMax をこの式で取り直し、5.104 の Σshare ＋ RESERVE ≤ MemTotal を確かめる — それでも自分の
DA 応答は attempt と seat の空きを待つ（旧 R-2）。

### 2.4 host ごとの算術（MemTotal 23.47 GiB = 24,033 MiB、3 台とも同じ）

「宣言」は preflight が確かめる Σshare ＋ RESERVE、「物理の最悪」は全 node が share いっぱいに仕事を走らせたときの見積もり
（anon ＝ base ＋ ΣW、artifact は host の page cache で 1 回、kaspad 以外）。

| host | 宣言（Σshare ＋ RESERVE） | 物理の最悪（見積もり） | MemoryMax の和（参考。分割ではない） |
|---|---|---|---|
| ibm | 10,496 ＋ 8,192 ＋ 2,048 = **20,736**（残り 3,297） | b0 6,089（base 784 ＋ attempt と seat 2 本 5,305）＋ b1 4,234（668 ＋ seat 2 本 3,566）＋ artifact 1,717 ＋ 他 300 ≈ **12.1 GiB** | 36G |
| .113 | 8,192 ＋ 4,096 = **12,288**（残り 11,745） | b6 4,746（1,180 ＋ 3,566）＋ artifact 1,717 ＋ 他 1,300 ＋ postgres の cache ≈ **7.6 GiB ＋ postgres** | 17G |
| 5.104 | 5 × 3,584 ＋ 4,096 = **22,016**（残り 2,017） | 5 × 2,220（437 ＋ seat 1 本 1,783）＋ artifact 1,717 ＋ 他 300 ＋ t11 fixture node ≈ **12.8 GiB ＋ fixture** | 45G |

- 5.104 の RESERVE 4,096 は別 session の **t11 fixture node** を含む（旧 1,536 は含んでいなかった — review）。preflight はその RSS を
  表示し、RESERVE から 1.5 GiB を残せなければ警告する（止めてもらうか RESERVE を上げる）。switch は MemAvailable ≥ Σshare ＋ 1 GiB を要求。
- PALW の仕事で host が OOM する経路は無い（ledger は予約ごとに artifact を数え、live 項でもう一度引くので保守的）。ledger の外で伸びるもの
  （IBD の cache、mempool、RPC）は ram-scale と MemoryMax が抑える。
- kaspad 本体の行は **stage の後の実測で埋める**: `install-<host>.sh check` が unit ごとに process RSS、cgroup の current / anon / file、
  ledger の live と available を出す（Mac の隔離 node は share 3.5 GiB・artifact 無し・fresh chain で RSS ≈ 95 MiB）。

### 2.5 残るリスク（公開前に node 側で直すか、既知として観測する）

- **R-2b（自分の DA 応答の余地は予約されていない — review の medium）**: b0・b1・b6 の share は「attempt ＋ 自分の DA 応答 ＋ seat 1 本」
  で取ったが、seat の replay は 2 本まで同時に走り、DA 応答には優先権も事前予約も無い（`IN_FLIGHT = 2`、DA 応答は拒否されると次の tick）。
  share の範囲で 2 本目の seat replay が先に入ると、自分の DA 応答は空きを待つ: b0 は attempt 3,456 ＋ seat 3,500 ＋ 2 本目 3,456 =
  10,412 ≤ 10,496 で DA 応答 3,456 が入らない。b1/b6 は seat 2 本 7,000 ≤ 8,192 で DA 応答 2,229 も attempt も入らない（9,229 > 8,192）。応答の期限は session の `W_disclose` = **1,200 DAA**（名目 120 s/DAA で 40 時間、実測
  ~200 s/DAA で約 2.8 日）で、replay は分単位なので窓は来るはずだが、保証は無い。答えられなければ正直な producer でも期限で S1 になる
  （IA-14 9-3 の「producer の V2 DA responder」はコード上あるが、運用で飢え得る）。
  - **node 側の修正（推奨、公開前）**: 自分の claim の DA duty が保留中は seat の 2 本目を始めない、または DA 応答に予約枠を先取りさせる。
    実装の lane の仕事（この kit は production code を変えない）。checklist §7 に open item として載せた。
  - 入らなければ **既知リスクとして公開**し、O-7 で監視する: journal の `da-answer` の ledger 拒否（`memory ledger cannot cover` と
    "Asked again next tick"）が自分の claim で続く時間、DA session の期限までの残り。期限の半分（600 DAA）を超えて答えていなければ
    実装の lane に上げる。panel を止めるのは解決にならない（DA の応答も panel の仕事なので、応答ごと止まる。09-25 から panel は
    duty で、止める flag 自体が無い — §14）。
- **R-3（seat のみの node の直列化）**: 3,584 MiB の seat は仕事を 1 本ずつしかできず、仕事の間は readiness proof も出ない（full seat の額の
  `can_reserve` が通らない）。8k の replay 時間 × 同時に振られる claim 数が receipt の期限を超えると Withheld / missing、readiness が
  readiness age（8 span）を越えて途切れると 8k の ready 7 を割る。公開後に O-2 の door histogram・replay 時間・readiness の `no proof`
  を確認する。
- **R-4（page cache の蓄積）**: §2.3 の cache 余裕 2,048 MiB を RocksDB・log・capture の page が越えると cgroup 項が share を下回る。
  `check` の警告で見つけ、その node を `-` にする。
- 数字は a0af3c92 の profile。A-held（object 57 の応答・held dissection）が merge されると 8k の court / DA の予約額が変わり得る。
  出荷 commit で worksheet を再実行し（checklist §4 step 6）、この表と install-*.sh の share / memmax を直す。
- ibm の disk（空き 16 GB）: P2-7 の retention janitor は `max(8 GiB, 5 %)` の空きを守るために、足りなければ attempt capture、次に
  foreign copy を消す（court に答える材料が減る）。§10 Q12 の旧モデル削除（ユーザー）を切替の前に。

## 3. ポート

| host | bond | P2P | gRPC | wRPC borsh | wRPC JSON | EVM | appdir | 既存の利用者 |
|---|---|---|---|---|---|---|---|---|
| ibm | 0 | **0.0.0.0:26311** | – | 26313 | 26314 | – | /root/.t12r-b0 | seeder（26313）、faucet（26313）、vantage tunnel（26314 → .113:28014 → misakascan /kaspa-hub）、各 seeder の anchor dial（:26311）— **どれも今は応答先が無い** |
| ibm | 1 | 0.0.0.0:26321 | – | 26323 | 26324 | – | /root/.t12r-b1 | 外部 peer（従来どおり） |
| .113 | 6 | **0.0.0.0:26311** | 26312 | 26313 | 26314 | 8545 | /root/.t12r-b6 | filler/REST（26312）、seeder・t11 validator（26313）、nginx misaka_json と wallet の /kaspa（26314）、nginx /evm（8545） |
| 5.104 | 2 | 127.0.0.1:26311 | – | 26313 | 26314 | – | /root/.t12r-b2 | seeder（26313） |
| 5.104 | 3/4/5/7 | 127.0.0.1:26321/31/41/51 | – | 263x3 | 263x4 | – | /root/.t12r-bN | — |

gRPC は .113 以外 `--nogrpc`（同一 host 2 node で既定ポートが衝突するため）。appdir は **新しいパス**（`/root/.t12r-bN`）なので旧 chain の再開は起こり得ない。旧 appdir は switch が名前を変えて退避する。
5.104 の 26511–26543（t12f）・27501–27596（t12p/scan/daa-obs）は別 session のもので触らない。

## 4. リリース値の埋め方（`fleet.env`）

0. Mac で一度だけ `cp fleet.env.example fleet.env`。`FORBIDDEN_GENESIS`（retired・私設・drill 1/2 の 6 本、全桁）と 8k artifact の値は
   記入済み。`HB_ADDR` は card 0 の payout address（旧 kit の `qf6hf5v0…`）を全桁で入れてある — 運用者が確認する（R3）。
1. 出荷する commit（checklist §4 の step 1〜8 を終えたもの）を origin に push（**ユーザー**）。
2. **Mac で先に identity を読む**: `./probe-identity-local.sh --build`（または `--kaspad <path>`）。隔離した node（127.0.0.1 のみ、
   peer 無し、DNS 無し、使い捨て appdir と HOME、`env -i`、PALW flag 無し）を起動して `EXPECT_FP`・`EXPECT_GENESIS`・
   `PREMINE_TXID` を読み、止める。値は profile や CPU に依らない（params と genesis の関数）。
3. **release build は Mac で（推奨、§13）**: `colima start` の後、出荷 commit の checkout で
   `PROBE_IMAGE=rust:latest NATIVE_PROBE=1 ./build-release-local.sh <commit>`。出荷 commit の clean な
   worktree（`git worktree add --detach`）から `cargo zigbuild --release --locked --target x86_64-unknown-linux-gnu.2.39` で
   5.104 版と同じ 4 本（kaspad（既定 feature = `evm` 込み）・misaka・palw-class・misaka-dnsseeder、1 回の呼び出し）を作り、
   `.cache/<rev12>/` に SHA256SUMS・REV・BUILD-INFO・ELF-CHECK（x86-64 ELF、interpreter `/lib64/ld-linux-x86-64.so.2`、NEEDED が glibc
   だけ、GLIBC_ の最大が 2.39 以下）を置く。source path は remap するので、同じ commit はどの directory で build しても同じ sha になる。続けて **release の kaspad そのもの** を、この Mac に既にある linux/amd64 の image
   （`PROBE_IMAGE`: glibc ≥ 2.39・bash・python3。`--pull never`・`--network none`）の中で colima の qemu で動かして隔離 probe し、
   `IDENTITY` を書く。`NATIVE_PROBE=1` で同じ commit の Mac 用 dev build も probe し、header（時刻・binary の path・sha256・source・log）
   以外の**全行**（3 値・schedule id・rule manifest digest・起動時の `court_e2e_root`・genesis の CLASS 行と BOND 行）の一致を確かめる
   （step 2 を兼ねる）。最後に貼る `fleet.env` の行を出す。リモートには触れない。**5.104 の MemAvailable にも公開 node にも触れない**。
   cargo は `env -i`（PATH・HOME・TMPDIR・CARGO_HOME/RUSTUP_HOME・target dir・remap・`CARGO_INCREMENTAL=0` だけ）で走り、commit の外の
   cargo config（`$CARGO_HOME/config.toml`、checkout より上の `.cargo/config.toml`）があれば止まる（`ALLOW_CARGO_CONFIG=1` なら build して
   BUILD-INFO に記録）。ELF の glibc 検査は script の定数 `FLEET_GLIBC=2.39` に対して行い、`GLIBC_FLOOR` がそれより上なら始めない。
   同じ WORK・target dir・`<rev12>` を使う run は同時に 1 本だけ（lock。生きている run が持っていれば 2 本目は何も触らずに止まる）。
   **フォールバック**（Mac が使えないとき）: 5.104 で `build-release-5104.sh <commit>` — `/root/t12-rel/src`（blobless clone）→
   `/root/t12-rel/incoming/<rev12>/`。最後に同じ隔離 probe（`env -i`・使い捨て HOME: root の `~/.misaka/testnet-12/endpoints.json` は
   live node のものなので書き換えない）で `IDENTITY` を出す。**Mac の probe と 3 値が一致しなければ止める**。
   **（ユーザー確認: 5.104 でのビルド開始）**
4. Mac の `fleet.env` に `REV`・`KASPAD_SHA256`・`MISAKA_SHA256`・`PALW_CLASS_SHA256`・`EXPECT_FP`・`EXPECT_GENESIS`・`PREMINE_TXID` を
   貼る。`SEEDER_SHA256` は既定 `KEEP`（§10 Q9）。genesis が a27f8f44 から動いた場合は a27f8f44 を `FORBIDDEN_GENESIS` に足す（c8652a97 の
   build はそれを走らせる）。公開後の drill は salt ごとに genesis が変わるので、drill を始める前に
   `./probe-identity-local.sh --drill-salt <salt>` が出す `DRILL_GENESES+=…` を足す。
5. 各 node の launch script は起動のたびに binary の sha256、「渡す flag を binary が全部知っているか」、「panel・round lane・chain classes を
   binary が always-on の duty として動かすか」（`--help` の `ALWAYS-ON DUTY` 表示、§14）、drill の flag・`KASPAD_*` 環境変数・
   drill marker が無いことを確認し、違えば **exit 78**（`RestartPreventExitStatus=78` なので crash loop しない。09-23 に ibm の公開 node が
   108 回 loop した事故の再発防止）。switch は node ごとに journal の `Consensus params fingerprint:` を `EXPECT_FP` と照合し、
   "PALW DRILL" を名乗れば止め、起動直後の `PALW duties` の 1 行で panel と round lane が ON であることを確かめ（idle なら警告）、
   RPC で `EXPECT_GENESIS` を確かめ、違えばその node を止めて後続を起動しない。

**8270cf03 での暫定値（出荷値ではない）**（`scripts/t12-repin.sh` が build から計算した値。`--apply` が値とこの表示の commit を、出荷 commit での `--apply --shipping` が表示を出荷値に書き換える。経緯は checklist §3）:
`EXPECT_FP=99eae89db05887c0ee21e451d5a78bd296533db59eb818edb3a4a320565c0ba3`、
`EXPECT_GENESIS=a27f8f44fe4d91a5…a8ca1f23`（全桁は checklist §3）、`PREMINE_TXID=5e0d5f1b37a71288…e55e2669`、
schedule id `5f53b691…`、rule manifest digest `9def81a1…`。

## 5. 手順（誰が・どの順で）

**【確認】** の付いた step は、実行の前に **ユーザーの明示の確認** を取る（公開 node の停止・置換、配備、告知、premine や運用者の鍵
を使う操作、公開ホストでの長いビルド）。kit のどの script もリモートで勝手には走らない。全体の順序（gate → 再 pin → battery 2 回 →
release build → fleet.env → 確認 → 配備 → explorer → seeder → 公開後の観測）は `docs/t12-rcore-launch-checklist.md` §4。

### Phase A — 準備（公開 chain は止まらない）
| step | 誰 | 何を |
|---|---|---|
| A0 | 実装・監査 → **ユーザー** | 出荷 commit を決める（checklist §4 step 1〜8: 未 merge の 4 本、再 pin、battery 2 回）。push は **【確認】** |
| A1 | ユーザー（Mac） | `./probe-identity-local.sh --build --layout` で出荷 commit の identity を読み、kit の写し（CLASS_8K・ART_8K_BYTES（sidecar と）・CLASS_2M_PREFIX・FEE_FLOAT_BASE・explorer の表）が binary と一致することを確かめる（exit 4 なら §5 の再 pin。リモートに触れない） |
| A2 | ユーザー（Mac） | `cp fleet.env.example fleet.env`（初回のみ）、`HB_ADDR`（card 0 の payout address が入っている）を確認 → `./distribute-from-mac.sh kit`（3 host の `/root/t12-rel/kit` に kit を置く）**【確認】** |
| A3 | ユーザー（Mac） | `PROBE_IMAGE=rust:latest ./build-release-local.sh <commit>`（§13: 空の target dir から 22〜32 分・-j4、container probe 12〜22 分。リモートに触れない）。`IDENTITY` が A1 の 3 値と一致すること。**フォールバック**: 5.104 で `cd /root/t12-rel/kit && ./build-release-5104.sh <commit>`（~21 分。MemAvailable ≥ 10 GiB が条件）**【確認】** |
| A4 | ユーザー（Mac） | `fleet.env` を埋める（A3 が最後に出す行）→ もう一度 `kit` → `binaries`（`.cache/<REV>` に Mac の build があれば 5.104・.113・ibm の 3 台へ、無ければ 5.104 の build を Mac 経由で .113・ibm へ。`BIN_SOURCE=local\|5104` で固定。**送る前に build の `IDENTITY` と `fleet.env` の `EXPECT_FP`・`EXPECT_GENESIS`・`PREMINE_TXID` を照合し、違えば何も送らない**。`IDENTITY` の無い build（`PROBE=skip`）は `IDENTITY_UNCHECKED=1` が要る。`IDENTITY` は binary と一緒に送られ、A5 の stage も同じ照合をする）→ `artifact`（8k は 09-24 に配置済みなら skip）**【確認】** |
| A5 | ユーザー（各 host） | `./install-<host>.sh preflight` → `./install-<host>.sh stage`（binary・artifact を sha 検証して `/root/t12-rel/<REV>/` に置き、launch script と unit を**その下に**書き、各 launch script の `--check` を通すだけ。/etc も稼働中 service も触らない）。preflight は drill node がホストで動いていれば NOT OK。stage は MemoryMax < share ＋ artifact ＋ 1 GiB の行を拒否 |

### Phase B — 切替（公開 chain の停止は ~10 分）— **B1〜B4 は公開 node を止めて置き換える: それぞれ【確認】**
| step | 誰 | 何を |
|---|---|---|
| B0 | **別 session** | 5.104 の `misaka-t12f-b2..b5`・`misaka-t12p-0..7`・`misaka-t12p-scan`・`misaka-t12p-scan-tunnel`・daa-obs node と `collect.py` を停止。`install-5104.sh switch` はこれらが 1 つでも動いていれば拒否する。鍵の削除はユーザー（rm コマンドを渡すだけ） |
| B1 | ユーザー（ibm）**【確認】** | 前提: §10 Q12 の旧モデル削除（disk）。`DISABLE_T11_NODE0=1 ./install-ibm.sh switch` — 旧 node1 停止 → drop-in/新 unit → `.t12`・`.t12b` 退避 → b0 起動（fp・"PALW DRILL" 無し・genesis・bond 8 本が `PREMINE_TXID:0..7`・`CLASS_8K` が登録済み・2M の prefix の class が登録済みを確認）→ 20 s → b1。b0 と b1 が互いを見た時点で heartbeat 開始 |
| B2 | ユーザー（.113）**【確認】** | 前提: `systemctl stop misaka-validator misaka-validator-2`（§10 Q7。**switch は動いていれば拒否する**）。`./install-113.sh switch` — 公開 node 停止 → b6（ibm と peer、heartbeat 2 本目） |
| B3 | ユーザー（5.104）**【確認】** | 前提: B0（別 session の node 停止。switch は拒否する）、t11 fixture node の RSS が RESERVE に収まる（preflight が表示）。`./install-5104.sh switch` — b2, b3, b4, b5, b7 を 30 s 間隔（各 node が 8k manifest を再導出） |
| B4 | ユーザー（.113）**【確認】** | explorer: `../misakascan-t12/DEPLOY.md` §9 の **どちらか一方**（運用者の決定待ち。推奨は `deploy.sh`）。`install-113.sh explorer-apply` は `deploy.sh` の `t12g.conf` があれば拒否し、`deploy.sh` は `zz-t12r.conf` があれば拒否する |
| B5 | ユーザー（Mac） | `./check-fleet.sh`（読み取りのみ: 全 node の fp・genesis・peer・lane mix・memory、seeder の応答、外からの 26311/26321 疎通）。1〜2 DAA 後に `CHECK_REGISTRY=1 ./check-fleet.sh` で 8k が `readySeatsNow ≥ 7` → Probation。2M は `ClassDeadlineUnmeasured`（4-quater が入っていれば）で閉じていること（O-11） |
| B6 | ユーザー（Mac） | seeder は触らない（`SEEDERS.md` §1）。切替後に `seeders/40-verify.sh`、続けて `CONFIRM=yes seeders/60-join-check.sh <5.104 上の release kaspad>` **【確認】**（5.104 で使い捨て node を 240 s 動かす。seat 5 本の横なので `env -i`・使い捨て HOME・`--ram-scale=0.1`、MemAvailable < 2 GiB なら起動しない） |
| B7 | **ユーザー【確認】** | 告知は B5・B6 が合格してから。「t12 は O-5 が合格するまで価値を持たない」（ADR §8.4）を含める |

合格条件: 8 node すべて fp 一致・genesis 保持・bond 8 本が `PREMINE_TXID:0..7`・`CLASS_8K` 登録済み・NRestarts 0・"PALW DRILL" 無し、
5.104 の各 node の peer ≥ 3、DAA が heartbeat で進む、30 分以内に lane mix の `attempt>0` と `receipt>0`（floor）、floor Final 後に
`round>0` と vesting 行（`misaka palw vesting`）、8k が Probation に入り 8k attempt が出る、各 node の reserved ≤ share、
**live（`memoryHeadroomBytes`）≥ share かつ `check` の cgroup 行に警告が無い**（§2.3）、journal に `memory ledger cannot cover` が続かない。

### Phase C — 受入れ後
- C1 **告知はユーザー【確認】**（旧 genesis の外部 node は handshake で `WrongGenesis` として拒否される — `flow_context.rs:1944`）。
- C2 `CONFIRM_PURGE=yes ./install-<host>.sh purge-old` で退避した旧 chain を削除 **【確認】**（ibm の disk のため。以後 rollback 不可）。
- C3 別 session が自分の t12f/t12p unit・`t12p.conf`・tunnel 鍵を片付ける。
- C4 DNS validator 6 × 20M MSK と faucet の資金は **運用者が premine の主ウォレットから**（§10、ユーザー決定。kit は鍵を持たない）**【確認】**。
- C5 公開後の観測 O-1〜O-13 と、後回しにした drill（`../t12-drill-kit/README.md`）。drill は公開 node の無いホストでだけ（R4）。

## 6. Rollback

host ごとに逆順（5.104 → .113 → ibm）: `./install-<host>.sh rollback`、.113 は先に `explorer-rollback`。
- drop-in を消す／新規 unit を disable+削除 → daemon-reload → 新 chain の appdir は `.rolledback-<ts>` として残す → **旧 appdir（`OLD_APPDIRS` だけ）を元の名前に戻す** → switch で disable した unit（ibm の t11 node0）を enable に戻す → **最初の switch 前に active だった unit だけ**起動（.113・ibm の公開 node。5.104 の旧 seat は failed のままにする — 旧 `c-seatN.sh` は旧 binary で crash loop するため）。
- 旧 script・旧 binary は一度も書き換えていないので、戻る先は切替直前の旧 chain（`fb1074b0…`、heartbeat のみの chain）そのもの。

## 7. ユーザーへの質問（09-23 時点の質問。回答は §10、R-core+ で新しく出た判断は §0 の R3・R4 と §2.3・§2.5）

- **Q1（B1・最重要）genesis 衝突**: 新公開 chain の genesis が private chain（`f6cc9576…`、同じ鍵・同じ premine、misakascan で公開済み）と同じになる。選択肢:
  (a) release で t12 genesis を変える（genesis header の timestamp／nonce、または premine の salt。fingerprint はどのみち変わるので告知コストは同じ）。**推奨**。
  (b) そのまま出す（`ALLOW_PRIVATE_GENESIS_REPLAY=1`）。private chain で使った fee float（`--palw-fee-outpoint 41..48` の quorum 提出）や第三者 bond への送金は誰でも公開 chain に再放送でき、各 node の fee outpoint が「使用済み」になって quorum 提出が止まる。private chain の data は削除しても、misakascan 経由で既に外に出ている。
- Q2 heartbeat は .113 b6 と ibm b0 の 2 本でよいか（1 本だと .113 停止で時計が止まる。3 本以上は block 数が増えるだけ）。
- Q3 floor producer を公開 host（.113 b6・ibm b1）に置いた。5.104 に寄せると宣言 share が 24 GiB（23.47 GiB の host）になる。公開 host の負荷を優先して .113 を seat のみ（share 3.5 GiB）にしてもよいか。
- Q4 8k producer を ibm b0（公開 host）にも置いた。09-23 の OOM を踏まえ unit に MemoryMax を付けてある。5.104 の 1 本だけにするか。
- Q5 ibm の `misaka-t11-node0` を disable してよいか（B5）。
- Q6 ibm の `misaka-faucet`（「testnet-11 faucet」、`FAUCET_RPC=127.0.0.1:26313`）は切替後 b0（新 t12）に繋がる。止めるか、新 t12 用に資金を入れるか。
- Q7 .113 の t11 DNS-finality validator 2 本（`--network testnet-11 --node-wrpc-borsh 127.0.0.1:26313`）は今も t12 node に繋いでいる。止めるか。
- Q8 explorer の JS（`/var/www/misaka-explorer/app.js` の PANEL_SEATS・LLM_CLASSES）は別 session が private roster（p0..p7、8k 追加）に書き換えたまま。card 0–7 は新 chain と同じなので中身は流用できるが、ラベルと誰が直すか（route-matrix #8 は「regenesis deploy と一緒に出す」）。kit は触らない。
- Q9 seeder は `KEEP`（1174b965、4 host 同一）でよいか。release に揃えるなら build が作る `misaka-dnsseeder` の sha を `SEEDER_SHA256` に入れ、`seeders/20-stage.sh` → `30-swap.sh` を 1 台ずつ（`SEEDERS.md` §5、追加 build 0 回）。seeder2/seeder4 は管理外の IP（217.76.57.217 / 217.178.101.111）に委任されたまま応答しない — 向け直すかは `SEEDERS.md` §6 Q2。
- Q10 producer の支払先を各 bond 鍵のアドレス（既定）にした。旧 ibm script は `misakatest:qf6hf5v0…` に払っていた。どちらにするか。
- Q11 `--palw-challenge`（licensed claim を全部再実行して court を開く）は全 node で off。1 seat だけ on にするか。
- Q12 ibm の disk（空き 16 GB）: `/root/palw-class` に `original-from-hf.palwq36` 36 GB・`huihui-30b.palwq36` 32 GB・gguf 23 GB と 18 GB がある。purge-old で空くのは ~1.6 GB だけ。
- Q13 DoS 修正 #4（held class の dense materialization をノード側で予算化）がメモリ項を変える可能性 — merge 後に private（または 5.104 の probe）で 8k replay 3.37 / 8k attempt 3.37 / floor 2.18 GiB を測り直してから share を確定するか。

## 8. 調査で分かったこと（09-23 22:xx、読み取りのみ）

**5.104.81.23**（vmi3272359、8 core、23 GiB、swap 19 GiB、disk 159 GB 空き、ufw INPUT DROP、load 8–10）
- 別 session が稼働中: `misaka-t12f-b2..b5`（旧 live chain の floor、`/root/t12-live/kaspad` sha **561d5b61**、ports 26511–26543、appdir `/root/.t12f-bN`、static＝boot 時は起動しない）、`misaka-t12p-0..7`（private、`/root/t12-private/bin/kaspad` sha **1dfb146e** = b38356fe、ports 27501–27573、keys `/root/t12-private/keys`）、`misaka-t12p-scan`（27581–27585）、`misaka-t12p-scan-tunnel`（→ .113 の 36312/36314/36545）、daa-obs node（27594–27596）と `collect.py`。各 private node の RSS ~2.6 GB。
- 旧 seat unit `misaka-t12-seat2..5` は **enabled・failed**（09-23 00:30 に stop timeout）。`/root/t12/kaspad` sha 113ca872、`kaspad.new` 6e5d512a。旧 appdir `.t12 .t12b .t12c .t12e`（各 2 MB 前後、鍵ファイル無し）。
- 鍵: `/etc/misaka/t12/t12-bond-{2,3,4,5,7}.key` と operator 同番号（各 64 B）。
- 8k artifact `/root/palw-class/qwen25-1.5b-a16-8k.palwart` 1,799,359,436 B、sha256 **b73600cf…**、sidecar は commit 済み manifest と一致（sha c3cc66bc…）。
- t11 fixture node（`/root/misakas-stale-consensus-diagnosis`、他 session）は放置。
- 注意: `/root/t12-private/src` は `/root/misakas` の worktree（origin = GitHub）。build script は触らず独自 clone。

**169.58.232.113**（vmi3527497、8 core、23 GiB、MemAvailable 10.1 GiB、swap 使用 4.2 GiB、disk 67 GB 空き、ufw 無効）
- `misaka-t12-node` active（09-22 23:58 から、NRestarts 0）、binary 561d5b61、**RssAnon 15.0 GiB**、旧 chain genesis は **`fb1074b0…`**（node log の最初の pruning point。doc の `a8cabac4…` は最初の配備時の値で、担保変更で premine と共に動いた）。
- `node.sh` は 2M class producer + heartbeat。`node.sh.with-share-for-deploy`（未使用の staged flag）は残置、`kaspad.new` 18531fdd も未使用。
- 鍵: `t12-bond-4/6`・`operator-4/6`（64/65 B）。8k は `.part` 54 MB のみ。
- seeder `/usr/local/bin/misaka-dnsseeder-t12` sha **1174b965**、`--listen 169.58.232.113:53 --anchors 169.58.232.113,169.58.39.220 --node-wrpc-borsh 127.0.0.1:26313`。
- misakascan は private chain を表示中: nginx `sites-enabled/misakascan` の 3 行（36314/36314/36545）、filler/REST の `t12p.conf`（gRPC 36312、DB `kaspa_t12p`）、app.js。filler は起動時に table を作る（`dbsession.py` の create_all）ので新 DB は `createdb` だけで足りる。
- t11 validator 2 本・MTP・miner pool・burn portal・postgres 16 が同居。

**ibm 169.58.39.220**（vmi3450148、8 core、23 GiB、swap 7.5 GiB、**disk 16 GB 空き・95 %**）
- `misaka-t12-node1` active（12:53 から — 09:52 の OOM と 108 回 loop の後）、binary 561d5b61、**RssAnon 14.7 GiB**、listen は 0.0.0.0:26321 だけ。unit は MemoryMax 24G。
- **26311/26313/26314 には何も居ない** → ibm の seeder（node 26313）、faucet（26313）、vantage tunnel（26314 → .113:28014 → /kaspa-hub）は応答先無し、各 seeder も ibm:26311 を dial できず、公開 DNS の seeder1 は .113 しか返さない（dig で確認）。b0 を 26311–26314 に置くのはこのため。
- `misaka-t11-node0` enabled・failed（`/root/t11/ibm-node0.sh` は 26311/26313）。
- 鍵: `t12-bond-0/1`・`operator-0/1`。8k は `.part` 37 MB のみ。旧 appdir `.t12`（1.6 GB、未使用）・`.t12b`（28 MB）。

**95.111.236.186**（vmi3225101、6 core、11 GiB）: seeder のみ（`/root/misaka-dnsseeder-t12` sha 1174b965、node RPC 26313 は応答先無し＝anchor のみ配布）。

**DNS**: seeder1 → .113 のみ、seeder3 → .113 と .220、seeder2/4 は委任無し。kaspad に焼き込まれた t12 の seeder は seeder1–4。

**全 host** x86_64 AMD EPYC・Ubuntu glibc 2.39 → 5.104 の 1 build を全 host で使える（今の .113/ibm も同じ binary を共有）。kaspad の既定 feature に `evm` が入っている（t12 は DAA 0 で EVM lane を有効化）。

## 9. kit の中身

| file | どこで | 何をする |
|---|---|---|
| `fleet.env.example` → `fleet.env` | 全部 | release 値（placeholder）、禁止 genesis（全桁 6 本）と `DRILL_GENESES`、定数、8k artifact の sha。`fleet.env` は gitignore（`*.env`）なので Mac で写して埋める |
| `lib.sh` | host | 共通: launch script/unit 生成（sha・flag 自己検査・duty 検査・drill flag / `KASPAD_*` 環境 / drill marker の拒否、exit 78）、preflight、stage、switch（fp・"PALW DRILL"・`PALW duties`・genesis の確認）、check、rollback、purge |
| `install-ibm.sh` / `install-113.sh` / `install-5104.sh` | 各 host | node 表（share は §2）・旧 appdir・host 固有の拒否条件（.113 は explorer-apply/rollback も） |
| `probe-identity-local.sh` | Mac | 出荷 commit の kaspad を隔離して起動し、`EXPECT_FP`・`EXPECT_GENESIS`・`PREMINE_TXID`（＋schedule id・rule manifest digest）を読み、genesis の class（id・artifact bytes・root）と bond outpoint を kit の写しと比べる（不一致は exit 4）。`--layout` で `t12_deploy_kit_constants`（premine の index 配置・class id・explorer の表）と 8k sidecar の sha も。`--drill-salt` で drill genesis も出す。リモートに触れない |
| `SEEDERS.md`・`seeders/` | Mac | seeder 専用の手順と script（変更なし）。`fleet.env` の `SEEDER_SHA256`・`EXPECT_FP` を読む |
| `t12check.py` | host / Mac | stdlib だけの JSON wRPC checker（fp・genesis・peer・lane mix・producer・memory・registry）。`--expect-premine`・`--expect-class`・`--expect-class-prefix` で kit の写しを確かめ（switch の gate と check が使う）、live が share を下回れば警告。`--probe` は隔離 node の fp/genesis、`--premine` は genesis bond の txid、`--layout` は genesis の class と bond |
| `build-release-local.sh` | Mac | **推奨の release build**（§13）: clean worktree → `cargo zigbuild`（x86_64 Linux、glibc 2.39 floor、`env -i`）→ `.cache/<rev12>/` に 4 本 + SHA256SUMS・REV・BUILD-INFO・ELF-CHECK → release の kaspad を linux/amd64 container で隔離 probe（`PROBE_IMAGE`、pull しない・network none）して IDENTITY → `fleet.env` の行。WORK・target dir・`<rev12>` ごとに lock。リモートに触れない |
| `build-release-5104.sh` | 5.104 | **フォールバック**: 独自 clone で release build → `incoming/<rev12>` + SHA256SUMS + 隔離 probe（`env -i`・使い捨て HOME）で IDENTITY |
| `distribute-from-mac.sh` | Mac | kit / binary / 8k artifact を Mac 経由で配布（host 間に鍵を足さない）、各段で sha 検証。`binaries` は Mac の build（`.cache/<REV>/BUILD-INFO`）があれば 3 host へ、無ければ 5.104 の build を .113・ibm へ |
| `check-fleet.sh` | Mac | 読み取りのみ: 全 host の check、`seeders/40-verify.sh`、公開 DNS、外からの P2P 疎通 |
| （tree の test）`consensus/core/tests/t12_deploy_kit_constants.rs` | Mac / CI | kit と explorer の写し（`FEE_FLOAT_BASE`・`CLASS_8K`・`ART_8K_BYTES`・`CLASS_2M_PREFIX`・`LLM_CLASSES`・`PANEL_BOND_TX`・`FORBIDDEN_GENESIS`）がこの build の genesis と一致することの pin |
| （tree の test）`kaspad/tests/t12_role_memory_figures.rs` | Mac | `#[ignore]` の worksheet: 仕事ごとの予約額と、install-*.sh の各行の share・MemoryMax の下限（§2） |

2026-09-25 にローカルで確認したこと: 全 script の `bash -n`（Mac の bash 3.2。shellcheck は未導入）、launch script 8 本の生成と
`--check`（a0af3c92 の dev build の kaspad で全 flag が既知）、drill salt（argv・ARGS への書き込み）・`KASPAD_PALW_DRILL_*`/`KASPAD_*` 環境・
drill marker・未知 flag（接頭辞 `--palw-produc`）・sha 不一致がそれぞれ exit 78、`require_release` が禁止 genesis・`DRILL_GENESES`・
短い fp・placeholder を拒否、2M の producer class / artifact を stage で拒否、`probe-identity-local.sh` の実走（§4 の暫定値）。
リモートでは何も実行していない。

## 10. 2026-09-24 決定と §7 の回答（オーケストレータ追記）

- **Q1 genesis 衝突 → (a)**。公開 t12 の premine txid をネットワークと genesis から導出して t12 固有にし、genesis の timestamp も変える。実装は wt-t12 の統合エージェント（item 12）。完了後に `FORBIDDEN_GENESIS` は f6cc9576 のまま、`EXPECT_GENESIS` に新しい値を入れる。
  - 旧 live chain（c746f07c）の `premine:41` は、切り替えで旧 chain を退役させるまで手数料だけで再放送できる。資金は bond 0 のアドレスに留まる。
- **heartbeat/clock**。H1・H2・H3・H5 を release に含める（ユーザー決定）。実装は別 branch `hb/t12-heartbeat` で進め、wt-t12 に取り込む。H4・H6 は後回し。
- **Mainnet Decision A**。
  - 本番の窓は現状値のまま。t12 は短縮 challenge 120 を維持する。
  - coinbase の使用可能判定は DAA 満期だけにする（item 13）。
- Q2: heartbeat は b0(ibm) と b6(.113) の 2 本（既定）。
- Q3/Q4: 配置は計画どおり（MemoryMax あり）。→ 2026-09-25 の release-prep review で 2 点変えた（§12）: 5.104 b2 を seat のみにした
  （8k producer は ibm b0 の 1 本）。MemoryMax は残したが、ledger の cgroup 項が share を下回らない値に上げた（crash guard であり分割ではない）。
  cache が溜まって cgroup 項が share を下回るなら `-`（MemoryMax=infinity）にする判断が要る **【確認】**。
- Q5: `DISABLE_T11_NODE0=1` で switch する（rollback で enable に戻る）。
- Q6: faucet は b0 の 26313 に自動でつながる。資金は公開後に運用者の鍵で入れる（ユーザー決定）。faucet の表示名 "testnet-11 faucet" は直す。
- **Q7: .113 の DNS validator 2 本は t12 用に直す**（ユーザー決定）。
  1. switch の前に `systemctl stop misaka-validator misaka-validator-2` を実行する。t11 設定のまま 26313 につながると network 不一致で再起動を繰り返すため。
  2. 公開後、運用者が各 validator 鍵の資金アドレスへ送金する。
  3. release の `kaspa-pq-validator bond --network testnet-12 --node-wrpc-borsh 127.0.0.1:26313 --validator-key /etc/misaka/validator/t11-validator{,-2}.key --amount <t12 の min_bond 以上> --unbonding-period-blocks <t12 の floor 以上>` で bond を張る。
  4. unit を `--network testnet-12 --stake-bond <新しい outpoint>` にして再開する。
- Q8: explorer の app.js は scratchpad/misakascan/app.js（route-matrix #8 を反映済み）を explorer-apply で配る。
- Q9: seeder は KEEP。
- Q10: producer の支払先は各 bond 鍵のアドレス（既定）。
- Q11: `--palw-challenge` は off（既定）。
- **Q12: ibm の旧モデル 4 ファイル（約 111 GB）はユーザーが切り替え前に削除する**。どのプロセスも使っていないことは確認済み。
- Q13: DoS 修正（#4 の dense cap）を merge した後、release binary でメモリを測り直してから share を確定する。
- **bond を mainnet 想定の値にする**（2026-09-24、ユーザー決定。統合エージェントの item 14）
  - miner（producer）の下限は 13,000 MSK。option A の floor claim を同時に 1 本持てる最小額。
  - panel（seat）の下限は 130,000 MSK。genesis の 8 席（各 939,063 MSK）は満たしている。
  - DNS validator の最小 bond は 20,000,000 MSK。発動条件は validator 6 人以上、かつ active stake 1.2 億 MSK 以上。
- **DNS validator の運用**
  - finality を動かすには 6 本 × 20M MSK が要る。資金は運用者が premine の主ウォレットから入れる。
  - 今あるのは .113 の 2 本だけで、あと 4 本分の鍵と置き場所を決める必要がある。
  - 6 本揃うまで DNS finality は動かない。coinbase は DAA 満期で使えるので、生存性には影響しない。
- bond の登録下限は、監査の #12（DoS 側）で「panel の下限」にしてあるのを「producer の下限（13,000 MSK）」に直すよう、監査セッションに依頼した。
- **B4 解消（2026-09-24 03:21 JST）**: 8k artifact を .113 と ibm に置いた。ファイル名は `qwen25-1.5b-a16-8k.palwart.incoming`、sidecar は `.palwmanifest.incoming`。sha256 は b73600cf…/c3cc66bc… で、fleet.env の値と一致。
  - 経路は 5.104 → 各ホストの直接 rsync（ssh agent 転送。鍵はホストに置かず、転送後に agent から外した）。Mac の上り回線は約 7 KB/s で使えなかった。
  - `stage` で sha を再確認し、正式名に昇格させる。
- **2026-09-24 の tree（feat/testnet-12-regenesis @ 50565f55、battery 実行中）に伴う変更**
  - t12 の genesis は `d73dbf44…`（utxo commitment `12df48ae…`）、genesis 時刻は 2026-09-01T00:00:00Z。
  - premine と community の出力は t12 固有の txid（`5e0d5f1b…`）に移った。index は変わらない。
  - そのため、lib.sh と fleet.env で `--palw-producer-bond` / `--palw-fee-outpoint` を旧 txid（`6d6973616b612d7072656d696e65…`）で書いている箇所は、すべて新 txid に置き換える。release の probe で得た値を使う。
  - `FORBIDDEN_GENESIS` は f6cc9576 のまま。`EXPECT_GENESIS` には d73dbf44… の全桁を入れる。
  - 新 fence `palw_clock_floor`（H3/H5）が入って t12 の fingerprint も変わる。最終値は battery と R-core+ の merge の後に確定する。
  - 新しい担保モデル R-core+（ADR-0152）を genesis から有効にする。その実装が入ってから release をビルドする。

## 11. R-core+ に合わせた kit の変更（2026-09-25、`rcore/release-prep`）

- **flag**: kit が渡す flag はすべて a0af3c92 の `kaspad --help` にある（`--palw-chain-classes` は t12 では既定で ON になったが、
  flag 検査のために明示のまま）。消した flag は無い。**→ 09-25 に改訂: `--palw-panel`・`--palw-round-lane`・`--palw-chain-classes` は
  kit から消した（duty になった。§14）。**P2-9（carrier の relay）は flag を足していない。P2-12 の
  `--palw-drill-genesis-salt` とその他の `--palw-drill-*` は公開 kit に **一度も現れない**。launch script はそれらを見つけると exit 78。
- **bond / fee float の txid** は `fleet.env` の `PREMINE_TXID` に一本化した（旧 sentinel txid `6d697361…` は残っていない）。
  `build_args` は `<128 hex>:<index>` 以外を拒否する。
- **環境**: drop-in は `Environment=` と `EnvironmentFile=` を空にし、launch script は `KASPAD_*`・`MISAKA_PALW_*`・`PALW_*` の
  環境変数を拒否する（kaspad は ~100 個の `KASPAD_*` を読む。drill の knob の多くに環境の双子がある）。
- **genesis**: `FORBIDDEN_GENESIS` を全桁 6 本にし、`DRILL_GENESES`（公開後の drill の salt ごと）を足した。
  `ALLOW_PRIVATE_GENESIS_REPLAY` の抜け道は削除（§10 Q1 で決定済み）。`switch` は RPC で genesis も確認する。
- **memory**: §2 の算術に置き換えた（ibm b0 10,496・b1 8,192、.113 b6 8,192、5.104 は据え置き）。→ §12 で ledger の cgroup 項を入れて改訂。
- **2M**: 公開時点で閉じる。2M の class / artifact を指定した node は stage で拒否。
- **probe**: `probe-identity-local.sh`（Mac）を追加。`build-release-5104.sh` の probe も `env -i`・使い捨て HOME・`--outpeers=0`・
  `PREMINE_TXID` の出力・"PALW DRILL" の拒否を足した。

## 12. release-prep review の修正（2026-09-25）

- **memory（high）**: §2 を ledger の実際の式（`min(share, min(MemAvailable, memory.max − memory.current) − 1 GiB) − reserved`）で書き直した。
  MemoryMax を式から取り直した（ibm b0 20G・b1 16G、.113 b6 17G、5.104 の 5 seat 9G）。stage は `MemoryMax < share ＋ artifact ＋ 1 GiB`
  を拒否し、`check` は unit ごとに cgroup の current / anon / file と process RSS を出して cgroup 項が share を下回れば警告、`t12check` は
  ledger の live が share を下回れば警告。node 表の memmax に `-`（MemoryMax=infinity）を書けるようにした。
- **5.104 b2 は seat のみ**（review の medium: 自分の DA 応答の余地が予約されない）。R-2b は §2.5 に既知リスクとして書き、node 側の修正を
  checklist §7 の open item にした。
- **5.104 の RESERVE** を 1,536 → 4,096（t11 fixture node を含む）。preflight がその RSS を出し、switch は MemAvailable ≥ Σshare ＋ 1 GiB。
- **kit の写しの gate**（review の medium）: switch の gate と check は fp・genesis に加えて bond 8 本（`PREMINE_TXID:0..7`）、`CLASS_8K` の登録、
  `CLASS_2M_PREFIX` の class の登録を RPC で確かめる（registry の `artifactBytes` は work の導出値で file の大きさではないので、`ART_8K_BYTES`
  は tree の pin が committed sidecar と比べる）。`probe-identity-local.sh` も同じ比較をし、`--layout` で tree の pin test を走らせる。
- **explorer**: B4/B6 を DEPLOY §9 の決定待ちにし、`deploy.sh` と `explorer-apply` は互いの drop-in があれば拒否する。
- **.113 の validator**: `install-113.sh switch` は `misaka-validator` / `misaka-validator-2` が動いていれば拒否する。
- **60-join-check**: `env -i`・使い捨て HOME・`--ram-scale=0.1`・MemAvailable ≥ 2 GiB。コメントを switch 後の実態に直した。
- **HB_ADDR**: 旧 kit の `qf6hf5v0…` は card 0 の payout address だった。全桁を `fleet.env.example` に入れた（運用者が確認）。

## 13. release build は Mac で（2026-09-25、`rcore/release-build`）

**経路**: `build-release-local.sh <commit>`（Mac）→ `.cache/<rev12>/`（4 本 + SHA256SUMS・REV・BUILD-INFO・ELF-CHECK・IDENTITY）→
`distribute-from-mac.sh binaries`（BUILD-INFO があれば 5.104・.113・ibm の 3 台へ）。5.104 の `build-release-5104.sh` はフォールバック
（5.104 は公開 node を載せ OOM の履歴がある。Mac の build なら 5.104 の MemAvailable に依らない）。2 つの build を混ぜない:
`binaries` は `.cache/<REV>` に Mac の build があるのに 5.104 から取ろうとすると拒否する。fleet の sha256 は **出荷する方の build** の値。

**前提（この Mac に既にあるもの。この作業では何も入れていない・何も pull していない）**: rustup の `1.93.0`（rust-toolchain.toml の pin）に
`x86_64-unknown-linux-gnu` の std、zig 0.16.0（Homebrew。clang/lld 21.1.8）、cargo-zigbuild 0.23.4、brew の llvm（`llvm-readelf`）。
container probe には colima（vz・aarch64・`binfmt: true` = qemu-x86_64。既存 VM の起動に download は無かった）と、既に取ってある
`rust:latest` の linux/amd64（Debian 13、glibc 2.41、bash・python3）。無ければ `PROBE=skip` と `NATIVE_PROBE=1`（IDENTITY は同じ commit の
Mac 用 dev build の probe から。release の binary そのものは走らない — switch が node ごとに fp・genesis を再確認する）。

**同時実行・環境・glibc の固定（2026-09-25 のレビュー後）**: run は `$WORK.lock`・`<target dir>/.build-release-local.lock`・
`$CACHE/.<rev12>.lock` を最後まで持ち、生きている run が持っていれば 2 本目は何も触らずに止まる（checkout `$WORK/src` を共有した 2 本は
片方の build 中に source を別 commit に差し替え、2 つの commit が混ざった binary を片方の rev の名前で出しうる。target dir を共有すれば
互いの binary を拾い、`<rev12>` を共有すれば互いの staging を消す）。kill -9 等で残った死んだ run の lock は引き継ぐ。checkout は固定の
`$WORK/src`（remap がこの path を含むので、固定しておけば依存の build を再利用できる）で、この run が作ったときだけ終了時に消す。
cargo は `env -i` で走る: 呼び出し側の `CARGO_PROFILE_RELEASE_*`・`CC`/`CXX`/`CFLAGS`/`CXXFLAGS`（cc crate が rocksdb・secp256k1・blst・
c-kzg・ring・zstd の compile に読む）・`*_SYS`・`ROCKSDB_*`・`RUSTUP_TOOLCHAIN` は届かない。commit の外の cargo config
（`[build]`・`[profile]`・`[env]`・`[target]`）は止める（`ALLOW_CARGO_CONFIG=1` なら path と sha256 を BUILD-INFO に）。ELF の glibc
検査の基準は script の定数 `FLEET_GLIBC=2.39`（fleet の Ubuntu 24.04）で、zig の link 先を選ぶ `GLIBC_FLOOR` とは独立（2.41 などを
渡すと始まる前に止まる）。`NATIVE_PROBE=1` の照合は header 以外の全行（schedule id・rule manifest・`court_e2e_root`・CLASS・BOND を含む）で、
不一致は `probe-diff.txt` に出して止まる。native の dev build も `env -i`、`OFFLINE=1` なら `--offline`。

**再現性（path の remap）**: rustc は compile した source file の絶対 path を binary に埋める（panic の位置、`include!` した OUT_DIR の
生成物）。remap 無しでは同じ 8270cf03 を別の directory で build すると sha256 が変わり（`2543bd94…` と `ed45bbcd…`。違いは埋め込まれた
path だけ）、binary に build した人の home directory（`/Users/…`）が入る。script は `--remap-path-prefix` で checkout → `/misaka`、
target dir → `/target`、`~/.cargo` → `/cargo` に写す（`CARGO_ENCODED_RUSTFLAGS`。呼び出し側の RUSTFLAGS は捨てる）。**別の checkout・別の
target dir・空の cache からの 2 回の build で 4 本とも byte 一致した**（下の表）。同じ toolchain（rustc 1.93.0・zig 0.16.0・cargo-zigbuild
0.23.4）なら誰の Mac でも同じ sha になるはずで、監査が出荷 binary を作り直して照合できる。

**8270cf03 での実測**（**暫定**: 出荷 commit ではない。R1 の merge と再 pin で sha も identity も変わる）:

| | |
|---|---|
| build | `cargo zigbuild --release --locked --target x86_64-unknown-linux-gnu.2.39 --bin kaspad --bin misaka --bin palw-class --bin misaka-dnsseeder`、-j4。空の target dir から 31 分 53 秒（1 回目、他の lane が cargo を回していた）/ 22 分 30 秒（remap 付き、別の空の target dir）。依存 crate を再利用できる 2 回目以降は workspace と LTO だけで ~7〜20 分（4edf02ef の run は 8 分 12 秒、7d95a7e9 の run は 6 分 57 秒）。**build 設定の変更は不要**だった: librocksdb-sys（bindgen・C++）、ring、blst・c-kzg、secp256k1-sys、zstd/lz4/bzip2/libz、mimalloc（Linux では `override`）、kaspa-hashes の keccak asm、protoc（vendored）がそのまま通る。openssl は依存に無い |
| ELF | 4 本とも `ELF 64-bit LSB pie executable, x86-64`、stripped、interpreter `/lib64/ld-linux-x86-64.so.2`、NEEDED は `libc.so.6`・`libm.so.6`・`ld-linux-x86-64.so.2` だけ（C++ runtime は zig の libc++ を静的に。libstdc++・libgcc_s は要らない）。`/Users/` を含む文字列は 0 |
| glibc | 必要な最大 version: kaspad・misaka **GLIBC_2.39**（Rust std の `pidfd_spawnp`・`pidfd_getpid`。WEAK 参照だが version need は 2.39）、palw-class・misaka-dnsseeder 2.34。fleet は 2.39 なので可。floor を 2.39 より上げない |
| size | kaspad 65,688,280 B・misaka 26,691,264 B・palw-class 8,339,624 B・misaka-dnsseeder 8,038,672 B（remap 付き。path が短くなった分だけ remap 無しより小さい） |
| sha256（remap 付き、2 回一致） | kaspad `37dfea9d02e92d51ecb3de63c67bc1b64985ca6121bdfb00d983d974b838a823`、misaka `988c3aeb211fb0c4ca3163cba3910994c2a5c8a129a7aa94f3d5664ec81d99fc`、palw-class `7f76c0ab8d315881b3f11039c8a56db6cebdf19dd5da1621349d3f071460c9a5`、misaka-dnsseeder `666dcb31ba1297241f6ac225cb531936d4a48784759607fda11d9e6ff61943ab`（**暫定・配らない**） |
| identity | **release の kaspad そのもの** を `rust:latest` linux/amd64（qemu、`--network none`）で隔離 probe: `EXPECT_FP=99eae89db05887c0ee21e451d5a78bd296533db59eb818edb3a4a320565c0ba3`、`EXPECT_GENESIS=a27f8f44…a8ca1f23`（a0af3c92 から不変）、`PREMINE_TXID=5e0d5f1b…e55e2669`、schedule id `5f53b691…`、rule manifest `9def81a1…`、genesis class 3 本と bond 8 本。**Mac の dev build（arm64）の probe と全行一致**、起動時の court 自己検査の `court_e2e_root 9afbd2da…` も一致（x86_64 の zig build と arm64 の build が PALW の実行経路で同じ根を出す）。kit の写しの比較も exit 0 |
| 推奨の 1 コマンド | `PROBE_IMAGE=rust:latest NATIVE_PROBE=1 OFFLINE=1 ./build-release-local.sh 7d95a7e9`（kit だけ変えた commit。env -i・lock 入り）を通しで実行: build 417 s（依存を再利用）、container probe 6 分、native dev build 2 分 + probe 1 分。**4 本の sha256 は 8270cf03 の 2 回と byte 一致**（env -i にしても bytes は変わらない）。header 以外の 20 行（`COURT_E2E_ROOT=9afbd2da…` を含む）が native と一致し、`probe-diff.txt` は空。死んだ run の lock（pid 999999）は引き継ぎ、実行中に同じ WORK・同じ target dir・同じ `<rev12>` で始めた 3 本はどれも何も触らずに止まった。終了後に lock と `$WORK/src` は残っていない |
| probe 時間 | qemu の下で 6〜22 分（起動時の court 自己検査が native の ~20 倍。native dev build は ~1 分）。`PROBE_WAIT_S` の既定は container で 3600 s。起動直後の数秒は seat / registry の RPC が空を返すことがあり（qemu の下で 1 度、kit の写しの不一致として出た）、probe は `--premine`・`--layout` を 60 s まで取り直す |

**5.104 の build との違い**（どちらを出荷しても identity は同じ。sha は違う）:
- C/C++ 部分（rocksdb・secp256k1・blst/c-kzg・ring・mimalloc・keccak asm）の compiler が 5.104 の gcc/libstdc++ ではなく clang 21 と
  静的 libc++（zig）。Rust の部分（consensus・PALW の整数演算）は同じ rustc 1.93.0・同じ target・同じ baseline CPU（target-cpu 指定なし）で、
  codegen は build host に依らない。上の probe が fp・genesis・court_e2e_root の一致でそれを確かめた。
- glibc は zig が持つ 2.39 の stub に対して link する（5.104 は実 glibc 2.39 に link）。どちらも要求する version の上限は 2.39 で、
  ELF-CHECK が build ごとに確かめる。
- 5.104 の build は path を remap しない（`/root/t12-rel/src/…` が入る）。

**Mac でできなかったこと**: fleet と同じ OS（Ubuntu 24.04・glibc 2.39・EPYC）の上での実行。container は Debian 13（glibc 2.41）の qemu
（CPU model は qemu のもの）。fleet の上では `install-*.sh stage` が各 launch script の `--check`（binary の sha・flag 自己検査）を、
`switch` が node ごとの fp・genesis を確かめる。これが最初の実機実行になる。

**最終 build でやり直すこと**（出荷 commit が決まったら。checklist §4 step 10）: 出荷 commit の checkout（この kit を含む = `rcore/release-build`
が merge 済み）で `PROBE_IMAGE=rust:latest NATIVE_PROBE=1 ./build-release-local.sh <出荷 commit>`（`colima start` が要る。空の target dir
から ~30 分 + container probe 12〜22 分 + native dev build と probe ~15 分）→ IDENTITY が step 6 の再 pin の値と一致 → 出た `fleet.env` の
行を貼る → `distribute-from-mac.sh kit`・`binaries`。上の表の sha・fp は 8270cf03 のもので、**貼らない**。

**colima の副作用**: `colima start` は restart 方針が `unless-stopped` の container（この Mac では `misaka-searxng-core`）も起動する。
probe が終わったら `colima stop` で元の Stopped に戻す。

**運用者の判断（任意）**: container probe を速くするなら colima を Rosetta で動かす（`colima stop && colima start --vz-rosetta`。
colima.yaml の `rosetta` が true になる = 設定の変更。Rosetta はこの Mac に入っている）。変えなくても probe は通る（遅いだけ）。

## 14. node の duty は起動 option によらず on（2026-09-25、`rcore/duties-always-on`）

ユーザー方針: protocol に参加するために node が **しなければならない** ことは operator の flag ではなく、off にする手段も無い。

- **規則**（`kaspad/src/palw_duties.rs`）: duty は (a) その network の params が該当 fence を持ち（`Params` の accessor で読む。network の
  列挙はしない）、(b) node がその duty に要る identity を持つとき、常に動く。fence が現在の DAA で発火しているかは各 service が自分の
  loop で見る（round producer は round ごとに lane の status を読み、panel の各仕事は `palw_rcore_plus_active_at` などで gate される）。
  identity が無い node（seat の無い relay、鍵の無い公開 node、鍵だけで bond の無い node）は何も起動せず、**INFO 1 行**で理由を言う。panic しない。
- **always-on になった duty**:
  | duty | 条件（params） | identity |
  |---|---|---|
  | panel の seat 業務（receipt・SEAT-R replay・readiness proof・DA 応答・自動 filer・fee outpoint があれば carrier） | ConsensusV2 | `--palw-producer-key` ＋ `--palw-producer-bond`（初回 bond 登録なら鍵＋`--palw-register-bond`） |
  | execution lane の round block（ADR-0125） | `palw_execution_lane` fence が設定されている | `--palw-producer-key` ＋ `--palw-producer-bond` |
  | chain-registered class の arm（ADR-0067） | model registry が genesis から有効（t12） | — |
- **旧 flag**: `--palw-panel`・`--palw-round-lane`（と環境変数 `KASPAD_PALW_PANEL`・`KASPAD_PALW_ROUND_LANE`、config file の同名 key）、
  t12 での `--palw-chain-classes`（`=false` 含む）は **受け付けるが何もしない**。名指しされた flag ごとに WARN 1 行
  （`… is deprecated and does nothing …`、`false` なら「値は無視、duty は切れない」）。t11・mainnet（registry が genesis からでない＝fence）では
  `--palw-chain-classes` は従来どおり operator の opt-in。
- **起動時の 1 行**: `PALW duties (on by construction; no flag turns one off): panel seat duties ON as bond … | execution-lane round blocks ON
  as bond … | chain-registered classes armed (…)`。idle の duty はそこに理由が出る。
- **CLI の選択のまま残るもの**: identity と resource（鍵・bond・支払先/heartbeat 先・fee outpoint・artifact・memory share/budget・cache）、
  **生産するか**（`--palw-produce`・`--palw-producer-class`・`--palw-canonical-claims`）、heartbeat miner（address が要る）、自発的な監視役
  `--palw-challenge`（紛争ごとに bond を賭ける）、`*-devnet`・`--palw-drill-*`。
- **kit の変更**: `build_args` は 3 つの duty flag を渡さない（渡そうとすると stage で die）。旧 binary 互換のために残す案は採らない: kit は
  release ごとに binary の sha を固定し、launch script が新しい **duty 検査** で「`--help` が `--palw-panel`・`--palw-round-lane`・
  `--palw-chain-classes` を `ALWAYS-ON DUTY` と表示する binary」以外を exit 78 で拒否するので、flag を落としても duty が消える組み合わせ
  （新 script × 旧 binary）は起動しない。rolling の途中でも、旧 script は旧 binary、新 script は新 binary を名指しする（各 script は自分の
  release の `bin/kaspad` を sha 付きで指す）。`switch` は RPC が答えた後（＝全 service が組まれた後）に最後の `PALW duties` 行を読み、
  panel と round lane が ON であることを確かめ、さらに `getPalwNodeStatus.panelRunning` が true になるのを待つ（`t12check.py --expect-panel`、
  警告のみで止めない）。`check` も `--expect-panel` を渡し、`panel idle`・`execution lane idle`・`deprecated and does nothing`・
  `round lane is not produced`・`panel service disabled`・`NOT as planned`・`receipts only` を拾う。
- **計画と実際（review 後の修正）**: 起動時の `PALW duties` 行は service を組む前の**計画**。鍵ファイルが読めない・bond が parse できないなどで
  計画上 ON の duty が起動しなかったときは、service を組んだ直後に `PALW duties NOT as planned: …` の WARN を duty ごとに 1 行出す。kit は
  最後の `PALW duties` 行を読むので、この訂正を拾う。gossip inbox の取り合いのように worker 起動時にしか分からない失敗は `panelRunning` で拾う。
- **fee outpoint**: `--palw-fee-outpoint` は支払いへの持ち主の同意なので flag のまま（未確定として報告）。無い seat も duty は動くが chain に
  何も運ばない（DA 応答・filer・carrier なし）。起動時の行は `panel seat duties ON as bond … (receipts only: …)` になり、`switch` は警告する。
  kit の node は全て fee outpoint を持つ。
- **1 つの bond は同時に 1 つの process だけ（review の high）**: duty に off が無いので、bond の鍵と outpoint を渡した process はすべてその bond の
  seat 業務と round block を担う。同じ bond の process が 2 つ同時にあると同じ round permit に別の block で署名し、chain は bond ごと slash する
  （`RoundPermitEquivocated`）。standby・`--palw-register-class` の登録用 process・別 host の複製・appdir を消して再同期した node（旧 process が
  まだ動いている場合）はすべて「2 つ目」にあたる。round の署名記録（`palw-round-last-signed`）は appdir にあるので、appdir は消さずに移す。
  node は起動時に `PALW bond …: run it in exactly ONE process …` を出す（通常は INFO、`--palw-register-class` のときは WARN）。登録は
  `misaka model add`（稼働中の node の RPC に送るだけ）か、稼働中の node 自身に flag を足して再起動する形にする
  （`docs/palw-add-a-model-runbook.md` §5・`docs/palw-certify-a-new-model.md`・`docs/model-requests.md` を直した）。kit は 1 bond 1 node。
- **drill**（`scripts/misaka-palw-t12-rcore-drill.sh`）: seat の起動から `--palw-panel` を外し、header に「round lane も常に動く・1 bond 1 process」、
  d6（heartbeat だけの区間）に「permit が切れるまで待つか bond 鍵なしで再起動」、d7 の fresh node に「bond 鍵を持たない relay で」と書いた。
- ローカルで確認: 生成した launch script の `--check` が新 binary で OK、`ALWAYS-ON DUTY` を持たない binary（旧 binary の模擬）で
  `does not run --palw-panel as an always-on duty … refusing (exit 78)`。
- **pre-t12**（`MISAKA-wt-b/pret12/lib.sh`）はまだ 3 つの flag を渡している。新 binary でも起動する（名指しした flag ごとに WARN が 1 行出るだけで、duty は flag によらず動く）。
