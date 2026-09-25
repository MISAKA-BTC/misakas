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
| R3 | `HB_ADDR`（heartbeat の支払先）の全桁が手元に無い。旧 kit の `fleet.env` は未追跡で失われ、文書には `misakatest:qf6hf5v0…` と先頭しか無い。§10 Q10 | **運用者** |
| R4 | 公開後の drill ホストが無い。公開後は ibm・.113・5.104 のすべてが公開 t12 node を動かすので、drill kit はこの 3 台を拒否する（ADR §8.3 item 3「公開 node のあるホストで drill しない」）。95.111 は seeder ホスト（11 GiB、egress 制限）で、rcore drill script の host guard も拒否する | **運用者**（公開後の drill 用に 4 台以上を用意） |
| B3 | 5.104 で別 session の node が動いていれば `install-5104.sh switch` は拒否する（旧 live の floor seat・private chain・scan・daa-obs） | 別 session が停止 |
| B5 | ibm の `misaka-t11-node0` が enabled。`DISABLE_T11_NODE0=1` で switch（§10 Q5 決定済み） | ユーザー |
| B6 | misakascan は現在 private chain / 旧 chain を表示。`install-113.sh explorer-apply` で新 chain＋新 DB へ | ユーザー |

旧 B1（genesis 衝突）は 09-24 に (a) で決定済み（§10）: t12 の premine txid は t12 固有の `5e0d5f1b…`、genesis は a0af3c92 で
`a27f8f44…`。f6cc9576… 以下の旧・私設・drill の genesis はすべて `FORBIDDEN_GENESIS` に全桁で入れてあり、install は拒否する。
旧 B2（DoS 修正未 merge）は統合線に入った。旧 B4（8k artifact）は 09-24 に解消済み（§10）。

## 1. 配置と役割

8 bond 全部が 8k（class `ebf44d0a…`、Qwen2.5-1.5B graph-v7@8192）の panel seat。class の lifecycle は ready 7 seat が要るので余裕は 1。
2M class（`74c67e63…`）は **公開時点で閉じている**（ADR §8.3 item 7 の IA-12 修正、U-D1、O-11）: どの node も 2M の artifact を持たず、
2M の producer にもならない。kit は 2M の class を producer / artifact に指定した node を stage の時点で拒否する（`lib.sh` `build_args`）。

| host | bond | unit | 種別 | 役割 | heartbeat | share（§2） |
|---|---|---|---|---|---|---|
| ibm 169.58.39.220 | 0 | `misaka-t12-node0` | **新規 unit** | 8k producer + 8k seat + panel + round lane | ✔ | 10,496 MiB |
| ibm | 1 | `misaka-t12-node1` | drop-in | floor producer + 8k seat + panel + round lane | | 8,192 MiB |
| .113 169.58.232.113 | 6 | `misaka-t12-node` | drop-in | floor producer + 8k seat + panel + round lane + explorer backend | ✔ | 8,192 MiB |
| 5.104.81.23 | 2 | `misaka-t12-seat2` | drop-in | 8k producer + 8k seat + panel + round lane | | 7,168 MiB |
| 5.104 | 3 | `misaka-t12-seat3` | drop-in | 8k seat + panel + round lane | | 3,584 MiB |
| 5.104 | 4 | `misaka-t12-seat4` | drop-in | 同上 | | 3,584 MiB |
| 5.104 | 5 | `misaka-t12-seat5` | drop-in | 同上 | | 3,584 MiB |
| 5.104 | 7 | `misaka-t12-seat7` | **新規 unit** | 同上（card 7 は 09-23 に 5.104 で再鍵、鍵はこの host にしか無い） | | 3,584 MiB |
| 95.111.236.186 | — | `misaka-dnsseeder-t12` のみ | 変更なし | seeder3（seeder は 4 台とも変更不要 — `SEEDERS.md`） | | — |

- producer: floor 2 本（ibm b1・.113 b6）、8k 2 本（ibm b0・5.104 b2）。全 node が `--palw-round-lane`（permit を取りこぼさない）。
- fee float は `$PREMINE_TXID:(41+N)`（card 7 → 48）、bond は `$PREMINE_TXID:N`。producer の支払先は各 bond 鍵自身のアドレス（既定、§10 Q10）。
- **R-core+ が node に足した仕事（どれも flag は無い。`--palw-panel` と `--palw-fee-outpoint` を持つ node で自動で動き、§2 の ledger に予約を取る）**:
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

## 2. メモリ予算（R-core+ の resource profile から再導出、2026-09-25）

**規則**（`kaspad/src/palw_memory_ledger.rs`）: node は仕事ごとに「holding（artifact file の bytes）＋ role の resource profile の
working set」を 1 つの ledger に予約し、`need ≤ min(share, MemAvailable − 1 GiB) − 既予約` のときだけ始める。足りなければ待つ
（ブロックは拒否しない）。`--palw-host-memory-share` は node ごとの上限で、`--ram-scale` もここから導かれる（0.0375/GiB）。
共有される page cache の artifact を予約ごとに数えるので、ledger は物理 RSS より保守的。

**一つの仕事の予約額**（a0af3c92。`palw_resource_profile_v1` を t12 genesis の 8k 行・canonical (1023, 2)・A16-KV-i16（出荷値
`KV_STORAGE_SHIPPED_V1`）・8 thread・prefill run 64 で評価した値。8k artifact は 1,799,359,436 B = 1,716 MiB）:

| 仕事（ledger の role 名） | 導出 | MiB |
|---|---|---|
| 8k producer の attempt（`producer`） | 1,716 + working set 1,740（K/V 28 + trace scratch 1,707 + fold 3 + …） | **3,456** |
| 8k full seat の replay（SEAT-R `full-seat`）、court（`court`）、DA の応答（P2-7 `da-answer`、full seat の額） | 1,716 + 1,740 | **3,456** |
| 8k partial seat の segment（S1 `partial-seat`、5 seat の最悪 segment 3） | 1,716 + profile 3,590 のうち capture を streamed fold（~0.4 MiB）に置き換えた 1,777（`palw_partial_seat_streamed_need_v1`） | **3,493** |
| floor（BASE-0）の attempt / replay / DA 応答 | BASE-0 は resource profile を持たない → holding ＋ 512 MiB の推定。8k を持つ node では holding が 8k file になる（09-23 実測 2.18 GiB と一致） | **2,228** |
| P2-8b の replay filer（未 merge） | dense の whole capture を予約する。8k の canonical capture は 105,518,224 leaves でホストの上限 2^26 を超えるので **8k では走らない**（streamed routes の仕事）。floor では小さい | floor で ≤ 2,228 |
| 2M（閉じている） | 参考: 2.67 GiB の artifact ＋ producer / full seat の working set 9,980 MiB。partial seat は streamed fold でも K/V と opening で 1.75〜17.5 GiB | 使わない |

seat の replay は同時に 2 本まで（`PalwSeatReplaysV1::IN_FLIGHT`）。DA の応答は ledger が空くまで毎 tick 再試行する（preempt は無い）。

**node ごとの share**（「同時に走らせたい仕事の和 ＋ 端数」。最小 = producer の attempt と seat の仕事 1 本が同時に走れる額）:

| node | 同時に走らせる仕事 | 和 | share | MemoryMax | 最小（参考） |
|---|---|---|---|---|---|
| ibm b0（8k producer + seat） | attempt 3,456 ＋ 自分の claim の DA 応答 3,456 ＋ seat の仕事 3,493 | 10,405 | **10,496** | 14G | 6,949（旧 7,168） |
| ibm b1（floor producer + 8k seat） | attempt 2,228 ＋ 自分の floor claim の DA 応答 2,228 ＋ seat 3,493 | 7,949 | **8,192** | 11G | 5,721（旧 6,144） |
| .113 b6（同上） | 同上 | 7,949 | **8,192** | 11G | 5,721（旧 6,144） |
| 5.104 b2（8k producer + seat） | attempt 3,456 ＋ seat 3,493（DA 応答は空き待ち） | 6,949 | **7,168**（据え置き） | 10G | 6,949 |
| 5.104 b3 b4 b5 b7（seat のみ） | seat の仕事 1 本 3,493（2 本目の replay と covering signer の DA 応答は待つ） | 3,493 | **3,584**（据え置き） | 6G | 3,493 |

**host ごとの算術**（MemTotal 23.47 GiB = 24,033 MiB、3 台とも同じ）:

- ibm: 10,496 + 8,192 + reserve 2,048 = **20,736 MiB**（残り 3,297 MiB）。kaspad 以外の RSS は 0.3 GiB（09-23 実測）。
- .113: 8,192 + reserve 4,096 = **12,288 MiB**（残り 11,745 MiB）。kaspad 以外 1.3 GiB ＋ postgres の cache。
- 5.104: 7,168 + 4 × 3,584 + reserve 1,536 = **23,040 MiB**（残り 993 MiB）。5 node あるので理想額（b2 10,496、seat は replay 2 本分
  6,986）は入らない（最低でも 38 GiB 要る）→ **最小額のまま**。物理的には 8k artifact 1.68 GiB が page cache で 5 process に共有される。

**残るリスク（運用者の判断）**:
- **R-2（5.104 b2 の DA 応答）**: 5.104 b2 は attempt と seat の仕事で share を使い切るので、自分の 8k claim への DA 応答は両方が空いた
  窓を待つ。応答の期限は session の 1,200 DAA（≈ 40 時間）で、producer は attempt ごとに予約を返すので窓は来るはずだが、保証は無い。
  答えられなければ正直な producer でも期限で S1 になる（IA-14 の「producer の V2 DA responder」が守ろうとしている点）。
  選択肢: (a) このまま（O-7 で観測）、(b) 8k producer を ibm b0 の 1 本にし 5.104 b2 を seat のみ（3,584）にする、(c) 5.104 の seat を 1 本減らす（8k の ready 7 を割るので不可）。**推奨は (b)**（§10 Q4 の再確認）。
- **R-3（seat のみの node の直列化）**: 3,584 MiB の seat は仕事を 1 本ずつしかできない。8k の replay 時間 × 同時に振られる claim 数が
  receipt の期限を超えると Withheld / missing になる。公開後に O-2 の door histogram と replay 時間で確認する。
- 数字は a0af3c92 の profile。A-held（object 57 の応答・held dissection）が merge されると 8k の court / DA の予約額が変わり得る。
  出荷 commit で `probe-identity-local.sh` と同じ場所に `misaka-palw-base0` の profile を再評価し（checklist §4 step 6）、この表を直す。
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
   記入済み。`HB_ADDR` は運用者が全桁を入れる（R3）。
1. 出荷する commit（checklist §4 の step 1〜8 を終えたもの）を origin に push（**ユーザー**）。
2. **Mac で先に identity を読む**: `./probe-identity-local.sh --build`（または `--kaspad <path>`）。隔離した node（127.0.0.1 のみ、
   peer 無し、DNS 無し、使い捨て appdir と HOME、`env -i`、PALW flag 無し）を起動して `EXPECT_FP`・`EXPECT_GENESIS`・
   `PREMINE_TXID` を読み、止める。値は profile や CPU に依らない（params と genesis の関数）。
3. 5.104 で `build-release-5104.sh <commit>` — `/root/t12-rel/src`（blobless clone）→ `/root/t12-rel/incoming/<rev12>/`。最後に同じ
   隔離 probe（`env -i`・使い捨て HOME: root の `~/.misaka/testnet-12/endpoints.json` は live node のものなので書き換えない）で
   `IDENTITY` を出す。**Mac の probe と 3 値が一致しなければ止める**。**（ユーザー確認: 5.104 でのビルド開始）**
4. Mac の `fleet.env` に `REV`・`KASPAD_SHA256`・`MISAKA_SHA256`・`PALW_CLASS_SHA256`・`EXPECT_FP`・`EXPECT_GENESIS`・`PREMINE_TXID` を
   貼る。`SEEDER_SHA256` は既定 `KEEP`（§10 Q9）。genesis が a27f8f44 から動いた場合は a27f8f44 を `FORBIDDEN_GENESIS` に足す（c8652a97 の
   build はそれを走らせる）。公開後の drill は salt ごとに genesis が変わるので、drill を始める前に
   `./probe-identity-local.sh --drill-salt <salt>` が出す `DRILL_GENESES+=…` を足す。
5. 各 node の launch script は起動のたびに binary の sha256、「渡す flag を binary が全部知っているか」、drill の flag・`KASPAD_*` 環境変数・
   drill marker が無いことを確認し、違えば **exit 78**（`RestartPreventExitStatus=78` なので crash loop しない。09-23 に ibm の公開 node が
   108 回 loop した事故の再発防止）。switch は node ごとに journal の `Consensus params fingerprint:` を `EXPECT_FP` と照合し、
   "PALW DRILL" を名乗れば止め、RPC で `EXPECT_GENESIS` を確かめ、違えばその node を止めて後続を起動しない。

**a0af3c92 での暫定値**（2026-09-25 に Mac の dev build で probe。出荷値ではない — 上の R1 の merge で動く）:
`EXPECT_FP=8a4810231e4f54e7d62b57ef0c4bc7973b23ebb87b1943b6541c05190b2d5ee6`、
`EXPECT_GENESIS=a27f8f44fe4d91a5…a8ca1f23`（全桁は checklist §3）、`PREMINE_TXID=5e0d5f1b37a71288…e55e2669`、
schedule id `8b0ee13c…`、rule manifest digest `9def81a1…`。

## 5. 手順（誰が・どの順で）

**【確認】** の付いた step は、実行の前に **ユーザーの明示の確認** を取る（公開 node の停止・置換、配備、告知、premine や運用者の鍵
を使う操作、公開ホストでの長いビルド）。kit のどの script もリモートで勝手には走らない。全体の順序（gate → 再 pin → battery 2 回 →
release build → fleet.env → 確認 → 配備 → explorer → seeder → 公開後の観測）は `docs/t12-rcore-launch-checklist.md` §4。

### Phase A — 準備（公開 chain は止まらない）
| step | 誰 | 何を |
|---|---|---|
| A0 | 実装・監査 → **ユーザー** | 出荷 commit を決める（checklist §4 step 1〜8: 未 merge の 4 本、再 pin、battery 2 回）。push は **【確認】** |
| A1 | ユーザー（Mac） | `./probe-identity-local.sh --build` で出荷 commit の identity を読む（リモートに触れない） |
| A2 | ユーザー（Mac） | `cp fleet.env.example fleet.env`（初回のみ）、`HB_ADDR` を記入 → `./distribute-from-mac.sh kit`（3 host の `/root/t12-rel/kit` に kit を置く）**【確認】** |
| A3 | ユーザー（5.104） | `cd /root/t12-rel/kit && ./build-release-5104.sh <commit>`（~21 分。MemAvailable ≥ 10 GiB が条件）**【確認】**。`IDENTITY` が A1 の 3 値と一致すること |
| A4 | ユーザー（Mac） | `fleet.env` を埋める → もう一度 `kit` → `binaries` → `artifact`（8k は 09-24 に配置済みなら skip）**【確認】** |
| A5 | ユーザー（各 host） | `./install-<host>.sh preflight` → `./install-<host>.sh stage`（binary・artifact を sha 検証して `/root/t12-rel/<REV>/` に置き、launch script と unit を**その下に**書き、各 launch script の `--check` を通すだけ。/etc も稼働中 service も触らない）。preflight は drill node がホストで動いていれば NOT OK |

### Phase B — 切替（公開 chain の停止は ~10 分）— **B1〜B4 は公開 node を止めて置き換える: それぞれ【確認】**
| step | 誰 | 何を |
|---|---|---|
| B0 | **別 session** | 5.104 の `misaka-t12f-b2..b5`・`misaka-t12p-0..7`・`misaka-t12p-scan`・`misaka-t12p-scan-tunnel`・daa-obs node と `collect.py` を停止。`install-5104.sh switch` はこれらが 1 つでも動いていれば拒否する。鍵の削除はユーザー（rm コマンドを渡すだけ） |
| B1 | ユーザー（ibm）**【確認】** | `DISABLE_T11_NODE0=1 ./install-ibm.sh switch` — 旧 node1 停止 → drop-in/新 unit → `.t12`・`.t12b` 退避 → b0 起動（fp・"PALW DRILL" 無し・genesis を確認）→ 20 s → b1。b0 と b1 が互いを見た時点で heartbeat 開始 |
| B2 | ユーザー（.113）**【確認】** | 先に `systemctl stop misaka-validator misaka-validator-2`（§10 Q7）。`./install-113.sh switch` — 公開 node 停止 → b6（ibm と peer、heartbeat 2 本目） |
| B3 | ユーザー（5.104）**【確認】** | `./install-5104.sh switch` — b2, b3, b4, b5, b7 を 30 s 間隔（各 node が 8k manifest を再導出） |
| B4 | ユーザー（.113）**【確認】** | `./install-113.sh explorer-apply` — b6 が check を通ることを確認してから nginx 3 行、filler/REST に `zz-t12r.conf`（gRPC 26312・新 DB `kaspa_t12r`）、`createdb`、再起動。app.js は `../misakascan-t12/DEPLOY.md` |
| B5 | ユーザー（Mac） | `./check-fleet.sh`（読み取りのみ: 全 node の fp・genesis・peer・lane mix・memory、seeder の応答、外からの 26311/26321 疎通）。1〜2 DAA 後に `CHECK_REGISTRY=1 ./check-fleet.sh` で 8k が `readySeatsNow ≥ 7` → Probation。2M は `ClassDeadlineUnmeasured`（4-quater が入っていれば）で閉じていること（O-11） |
| B6 | ユーザー（Mac） | seeder は触らない（`SEEDERS.md` §1）。切替後に `seeders/40-verify.sh`、続けて `CONFIRM=yes seeders/60-join-check.sh <5.104 上の release kaspad>` **【確認】**（5.104 で使い捨て node を 240 s 動かす） |
| B7 | **ユーザー【確認】** | 告知は B5・B6 が合格してから。「t12 は O-5 が合格するまで価値を持たない」（ADR §8.4）を含める |

合格条件: 8 node すべて fp 一致・genesis 保持・NRestarts 0・"PALW DRILL" 無し、5.104 の各 node の peer ≥ 3、DAA が heartbeat で進む、
30 分以内に lane mix の `attempt>0` と `receipt>0`（floor）、floor Final 後に `round>0` と vesting 行（`misaka palw vesting`）、8k が
Probation に入り 8k attempt が出る、各 node の reserved ≤ share、journal に `memory ledger cannot cover` が続かない。

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

## 7. ユーザーへの質問（09-23 時点の質問。回答は §10、R-core+ で新しく出た判断は §0 の R3・R4 と §2 の R-2）

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
| `lib.sh` | host | 共通: launch script/unit 生成（sha・flag 自己検査・drill flag / `KASPAD_*` 環境 / drill marker の拒否、exit 78）、preflight、stage、switch（fp・"PALW DRILL"・genesis の確認）、check、rollback、purge |
| `install-ibm.sh` / `install-113.sh` / `install-5104.sh` | 各 host | node 表（share は §2）・旧 appdir・host 固有の拒否条件（.113 は explorer-apply/rollback も） |
| `probe-identity-local.sh` | Mac | 出荷 commit の kaspad を隔離して起動し、`EXPECT_FP`・`EXPECT_GENESIS`・`PREMINE_TXID`（＋schedule id・rule manifest digest）を読む。`--drill-salt` で drill genesis も出す。リモートに触れない |
| `SEEDERS.md`・`seeders/` | Mac | seeder 専用の手順と script（変更なし）。`fleet.env` の `SEEDER_SHA256`・`EXPECT_FP` を読む |
| `t12check.py` | host / Mac | stdlib だけの JSON wRPC checker（fp・genesis・peer・lane mix・producer・memory・registry）。`--probe` は隔離 node の fp/genesis、`--premine` は genesis bond の txid |
| `build-release-5104.sh` | 5.104 | 独自 clone で release build → `incoming/<rev12>` + SHA256SUMS + 隔離 probe（`env -i`・使い捨て HOME）で IDENTITY |
| `distribute-from-mac.sh` | Mac | kit / binary / 8k artifact を Mac 経由で配布（host 間に鍵を足さない）、各段で sha 検証 |
| `check-fleet.sh` | Mac | 読み取りのみ: 全 host の check、`seeders/40-verify.sh`、公開 DNS、外からの P2P 疎通 |

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
- Q3/Q4: 配置は計画どおり（MemoryMax あり）。
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
  flag 検査のために明示のまま）。消した flag は無い。P2-9（carrier の relay）は flag を足していない。P2-12 の
  `--palw-drill-genesis-salt` とその他の `--palw-drill-*` は公開 kit に **一度も現れない**。launch script はそれらを見つけると exit 78。
- **bond / fee float の txid** は `fleet.env` の `PREMINE_TXID` に一本化した（旧 sentinel txid `6d697361…` は残っていない）。
  `build_args` は `<128 hex>:<index>` 以外を拒否する。
- **環境**: drop-in は `Environment=` と `EnvironmentFile=` を空にし、launch script は `KASPAD_*`・`MISAKA_PALW_*`・`PALW_*` の
  環境変数を拒否する（kaspad は ~100 個の `KASPAD_*` を読む。drill の knob の多くに環境の双子がある）。
- **genesis**: `FORBIDDEN_GENESIS` を全桁 6 本にし、`DRILL_GENESES`（公開後の drill の salt ごと）を足した。
  `ALLOW_PRIVATE_GENESIS_REPLAY` の抜け道は削除（§10 Q1 で決定済み）。`switch` は RPC で genesis も確認する。
- **memory**: §2 の算術に置き換えた（ibm b0 10,496・b1 8,192、.113 b6 8,192、5.104 は据え置き）。
- **2M**: 公開時点で閉じる。2M の class / artifact を指定した node は stage で拒否。
- **probe**: `probe-identity-local.sh`（Mac）を追加。`build-release-5104.sh` の probe も `env -i`・使い捨て HOME・`--outpeers=0`・
  `PREMINE_TXID` の出力・"PALW DRILL" の拒否を足した。
