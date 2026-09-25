# testnet-12 再 genesis — 4 ホスト配備計画（新 genesis）

作成: 2026-09-23（5a559459 時点・リモートは読み取りのみで調査）。**この kit はまだ何も実行していない。**
リリース固有値（binary sha256・fingerprint・genesis）は DoS 修正の merge 後に確定するため `fleet.env` では
`__FILL_ME__`。これが残っている限り `stage` / `switch` は動かない。

---

## 0. 公開を止めているもの（先に読む）

| # | 何か | 誰が |
|---|---|---|
| **B1** | **新 genesis が PRIVATE t12 と同一**。5a559459 の t12 genesis は `f6cc9576…`（b791b460 で確定、5a559459 は genesis を動かさない）。5.104 の route-matrix session の private chain（b38356fe、`misaka-t12p-*`）も p0.log の `Importing the UTXO set of the pruning point f6cc9576…` で**同じ genesis**。鍵（bond 0–7）も premine も network 名も同じ。PALW の署名 domain は `network名 ‖ genesis`（`palw_network_domain_v2_for`）なので private chain 上の署名済み object（receipt・登録・readiness 等）はすべて公開 chain で有効。さらに premine を使った取引（panel が quorum 提出で使う fee float `premine:41..48`、第三者 bond への送金）は**そのまま再放送できる**。private chain は 09-23 14:50 から misakascan.com で公開中。`fleet.env` の `FORBIDDEN_GENESIS` にこの値を入れてあり、install は拒否する。 | **ユーザー判断**（§7 Q1） |
| B2 | DoS 修正（別 session の #1–#4・#5 没収・#6–#8・#11–#14）が未 merge → release binary・sha・fingerprint 未確定 | 別 session → ユーザー |
| B3 | 5.104 で別 session の node が稼働中（旧 live の floor seat 4 本 + private chain 8 本 + scan + daa-obs、RSS 合計 ~25 GB、23 GiB の host）。新 chain と同じ鍵 | 別 session が停止 |
| B4 | 8k artifact が ibm と .113 に無い（どちらも 13:59 に止まった `.part` だけ）。7 ready seat には 8 bond 全部が 8k を持つ必要がある（余裕 1） | ユーザーが `distribute-from-mac.sh artifact` |
| B5 | ibm の `misaka-t11-node0` が **enabled**（failed 状態）。再起動時に 26311/26313 を取り b0 と衝突 | ユーザー判断（`DISABLE_T11_NODE0=1`） |
| B6 | misakascan は現在 private chain を表示中（nginx 3 行・filler/REST drop-in・app.js）。`install-113.sh explorer-apply` で新 chain + 新 DB へ | ユーザー（app.js は §7 Q8） |

---

## 1. 配置と役割

8 bond 全部が 8k（class `ebf44d0a…`、Qwen2.5-1.5B graph-v7@8192）の panel seat。class の lifecycle は ready 7 seat が要るので余裕は 1。
2M class（`74c67e63…`）は設計どおり **誰も持たない**＝Prefetching のまま。

| host | bond | unit | 種別 | 役割 | heartbeat |
|---|---|---|---|---|---|
| ibm 169.58.39.220 | 0 | `misaka-t12-node0` | **新規 unit** | 8k producer + 8k seat + panel + round lane | ✔ |
| ibm | 1 | `misaka-t12-node1` | drop-in | floor producer + 8k seat + panel + round lane | |
| .113 169.58.232.113 | 6 | `misaka-t12-node` | drop-in | floor producer + 8k seat + panel + round lane + explorer backend | ✔ |
| 5.104.81.23 | 2 | `misaka-t12-seat2` | drop-in | 8k producer + 8k seat + panel + round lane | |
| 5.104 | 3 | `misaka-t12-seat3` | drop-in | 8k seat + panel + round lane | |
| 5.104 | 4 | `misaka-t12-seat4` | drop-in | 同上 | |
| 5.104 | 5 | `misaka-t12-seat5` | drop-in | 同上 | |
| 5.104 | 7 | `misaka-t12-seat7` | **新規 unit** | 同上（card 7 は 09-23 に 5.104 で再鍵、鍵はこの host にしか無い） | |
| 95.111.236.186 | — | `misaka-dnsseeder-t12` のみ | 変更なし | seeder3（seeder は 4 台とも変更不要 — `SEEDERS.md`） | |

- producer: floor 2 本（ibm b1・.113 b6）、8k 2 本（ibm b0・5.104 b2）。全 node が `--palw-round-lane`（permit を取りこぼさない）。
- fee float は `premine:(41+N)`（card 7 → 48）。producer の支払先は各 bond 鍵自身のアドレス（`--palw-producer-pay-address` 省略時の既定）。heartbeat の支払先は従来どおり `misakatest:qf6hf5v0…`。
- `--enable-unsynced-mining` は使わない。heartbeat は peer さえあれば古い genesis timestamp でも打つ（`palw_heartbeat_miner.rs`、`should_mine` を通らない）ので、b0 が b1 を見た瞬間に時計が進み、producer の sync 判定も通る。
- 5.104 は ufw で INPUT DROP、全 node が 127.0.0.1 listen で .113/ibm に外向き接続（従来どおり）。

## 2. メモリ予算

実測（5.104 の private t12、09-23）: **8k full-seat replay 3.37 GiB**、**8k attempt 3.37 GiB**、**floor attempt 2.18 GiB**。
private の p1（floor producer + 8k seat、share 3.5 GiB）は readiness を **53 回中 49 回拒否**された（`less 2.18 GiB already reserved by producer of class f1c5635c…`）。
→ seat のみ ≥ 3.5 GiB、floor producer + seat ≥ 5.55 → **6 GiB**、8k producer + seat ≥ 6.74 → **7 GiB**（p0 は 7 GiB で両方通った）。

| host | MemTotal | node の share（`--palw-host-memory-share`） | share 計 | 予備 | 合計 | unit MemoryMax | 補足 |
|---|---|---|---|---|---|---|---|
| ibm | 23.47 GiB | b0 7 GiB / b1 6 GiB | 13 GiB | 2 GiB | 15 GiB | 10G / 9G | kaspad 以外の RSS 0.3 GiB。**disk 16 GB 空き（95 %）** |
| .113 | 23.47 GiB | b6 6 GiB | 6 GiB | 4 GiB | 10 GiB | 9G | kaspad 以外 1.3 GiB + postgres cache。いまの公開 node は RssAnon **15.0 GiB**（旧 binary・2M class 保持） |
| 5.104 | 23.47 GiB | b2 7 / b3・b4・b5・b7 3.5 GiB | 21 GiB | 1.5 GiB | 22.5 GiB | 10G / 6G×4 | 宣言は余裕 1 GiB。物理は下回る: artifact 1.68 GiB は page cache で 5 process 共有、private seat の RSS 実測 ~2.6 GiB（共有 file 込み） |
| 95.111 | 11 GiB | — | — | — | — | — | seeder のみ |

MemoryMax は share + 3 GiB 前後。暴走した node は自分の cgroup 内で落ち、公開 node を道連れにしない（09-23 の ibm OOM の教訓）。

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

1. DoS 修正 merge 後の commit を origin に push。
2. 5.104 で `build-release-5104.sh <commit>` — `/root/t12-rel/src`（blobless clone。`/root/misakas` とその worktree は触らない）→ `/root/t12-rel/incoming/<rev12>/`。最後に**隔離 probe**（127.0.0.1:26991/26994、peer 無し、使い捨て appdir、PALW flag 無し）で release 自身の fingerprint と genesis を読み、`IDENTITY` と貼り付け用の行を出す。genesis が `FORBIDDEN_GENESIS` なら警告。
3. Mac の `fleet.env` に `REV`・`KASPAD_SHA256`・`MISAKA_SHA256`・`PALW_CLASS_SHA256`・`EXPECT_FP`・`EXPECT_GENESIS` を貼る。`SEEDER_SHA256` は既定 `KEEP`（§7 Q9）。
4. 各 node の launch script は起動のたびに binary の sha256 と「渡す flag を binary が全部知っているか」を確認し、違えば **exit 78**（`RestartPreventExitStatus=78` なので crash loop しない。09-23 に ibm の公開 node が 108 回 loop した事故の再発防止）。switch は node ごとに journal の `Consensus params fingerprint:` を `EXPECT_FP` と照合し、違えばその node を止めて後続を起動しない。

## 5. 手順（誰が・どの順で）

### Phase A — 準備（公開 chain は止まらない）
| step | 誰 | 何を |
|---|---|---|
| A0 | **ユーザー** | §7 Q1（genesis 衝突）を決める。genesis を変えるなら A1 の commit に含める |
| A1 | 別 session → ユーザー | DoS 修正を merge・push |
| A2 | ユーザー（Mac） | `./distribute-from-mac.sh kit`（4 host の `/root/t12-rel/kit` に kit を置く） |
| A3 | ユーザー or 別 session（5.104） | `cd /root/t12-rel/kit && ./build-release-5104.sh <commit>`（~21 分。MemAvailable ≥ 10 GiB が条件、今は 17 GiB） |
| A4 | ユーザー（Mac） | `fleet.env` を埋める → もう一度 `kit` → `binaries` → `artifact`（8k 1.8 GB を ibm・.113 へ `.incoming` として。既存の `.part` は触らない） |
| A5 | ユーザー（各 host） | `./install-<host>.sh preflight` → `./install-<host>.sh stage`（binary・artifact を sha 検証して `/root/t12-rel/<REV>/` に置き、launch script と unit を**その下に**書くだけ。/etc も稼働中 service も触らない） |

### Phase B — 切替（公開 chain の停止は ~10 分）
| step | 誰 | 何を |
|---|---|---|
| B0 | **別 session** | 5.104 の `misaka-t12f-b2..b5`・`misaka-t12p-0..7`・`misaka-t12p-scan`・`misaka-t12p-scan-tunnel`・daa-obs node と `collect.py` を停止。できれば `/root/t12-private/keys`（ibm/.113 から複製した bond 0/1/6 の鍵）を削除。`install-5104.sh switch` はこれらが 1 つでも動いていれば拒否する |
| B1 | ユーザー（ibm） | `DISABLE_T11_NODE0=1 ./install-ibm.sh switch` — 旧 node1 停止 → drop-in/新 unit → `.t12`・`.t12b` 退避 → b0 起動（fp 確認）→ 20 s → b1。b0 と b1 が互いを見た時点で heartbeat 開始 |
| B2 | ユーザー（.113） | `./install-113.sh switch` — 公開 node 停止 → b6（ibm と peer、heartbeat 2 本目） |
| B3 | ユーザー（5.104） | `./install-5104.sh switch` — b2, b3, b4, b5, b7 を 30 s 間隔（各 node が 8k manifest を再導出） |
| B4 | ユーザー（.113） | `./install-113.sh explorer-apply` — b6 が check を通ることを確認してから nginx 3 行（36314→26314、36314→28014、36545→8545）、filler/REST に `zz-t12r.conf`（gRPC 26312・新 DB `kaspa_t12r`、DB URI は unit 自身の値から導出＝kit にパスワードを持たない）、`createdb`、再起動。別 session の `t12p.conf` は消さずに上書き |
| B5 | ユーザー（Mac） | `./check-fleet.sh`（全 node の fp・genesis・peer・lane mix・memory、seeder の応答、外からの 26311/26321 疎通）。1〜2 DAA 後に `CHECK_REGISTRY=1 ./check-fleet.sh` で 8k が `readySeatsNow ≥ 7` → Probation |
| B6 | ユーザー（Mac） | seeder は触らない（`SEEDERS.md` §1: binary は genesis に依存しない）。切替後に `seeders/40-verify.sh`、続けて `CONFIRM=yes seeders/60-join-check.sh <5.104 上の release kaspad>`（DNS だけで新 chain に届くかの e2e）。ibm の b0 が 0.0.0.0:26311 に立つので、今は死んでいる ibm の anchor も生き返る（`SEEDERS.md` §6 Q1 の「移す場合」） |
| B7 | **ユーザー** | 告知は B5・B6 が合格してから |

合格条件: 8 node すべて fp 一致・genesis 保持・NRestarts 0、5.104 の各 node の peer ≥ 3、DAA が heartbeat で進む、30 分以内に lane mix の `attempt>0` と `receipt>0`（floor）、floor Final 後に `round>0`、8k が Probation に入り 8k attempt が出る、各 node の reserved ≤ share。

### Phase C — 受入れ後
- C1 **告知はユーザー**（旧 genesis の外部 node は handshake で `WrongGenesis` として拒否される — `flow_context.rs:1944`）。
- C2 `CONFIRM_PURGE=yes ./install-<host>.sh purge-old` で退避した旧 chain を削除（ibm の disk のため。ただし以後 rollback 不可）。
- C3 別 session が自分の t12f/t12p unit・`t12p.conf`・tunnel 鍵を片付ける。

## 6. Rollback

host ごとに逆順（5.104 → .113 → ibm）: `./install-<host>.sh rollback`、.113 は先に `explorer-rollback`。
- drop-in を消す／新規 unit を disable+削除 → daemon-reload → 新 chain の appdir は `.rolledback-<ts>` として残す → **旧 appdir（`OLD_APPDIRS` だけ）を元の名前に戻す** → switch で disable した unit（ibm の t11 node0）を enable に戻す → **最初の switch 前に active だった unit だけ**起動（.113・ibm の公開 node。5.104 の旧 seat は failed のままにする — 旧 `c-seatN.sh` は旧 binary で crash loop するため）。
- 旧 script・旧 binary は一度も書き換えていないので、戻る先は切替直前の旧 chain（`fb1074b0…`、heartbeat のみの chain）そのもの。

## 7. ユーザーへの質問

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
| `fleet.env` | 全部 | release 値（placeholder）、禁止 genesis、定数、8k artifact の sha |
| `lib.sh` | host | 共通: launch script/unit 生成（sha・flag 自己検査、exit 78）、preflight、stage、switch、check、rollback、purge、seeder |
| `install-ibm.sh` / `install-113.sh` / `install-5104.sh` | 各 host | node 表・旧 appdir・host 固有の拒否条件（.113 は explorer-apply/rollback も）。seeder は `seeder-status`（読み取り）だけ |
| `SEEDERS.md`・`seeders/` | Mac | **別 agent が並行して書いた seeder 専用の手順と script**（読むだけで、この kit からは変更していない）。seeder の swap/rollback/検証はこちらに一本化し、node kit 側の重複（swap 機能と 95.111 用 script）は削除した。`fleet.env` の `SEEDER_SHA256`・`EXPECT_FP` を読む |
| `t12check.py` | host | stdlib だけの JSON wRPC checker（fp・genesis・peer・lane mix・producer・memory・registry）。`--probe` は隔離 node の fp/genesis を読む。旧 build の node で動作確認済み |
| `build-release-5104.sh` | 5.104 | 独自 clone で release build → `incoming/<rev12>` + SHA256SUMS + 隔離 probe で IDENTITY |
| `distribute-from-mac.sh` | Mac | kit / binary / 8k artifact を Mac 経由で配布（host 間に鍵を足さない）、各段で sha 検証 |
| `check-fleet.sh` | Mac | 読み取りのみ: 全 host の check、`seeders/40-verify.sh`、公開 DNS、外からの P2P 疎通 |

ローカルで確認したこと: 全 script の `bash -n`、launch script 生成と `--check`（Mac の kaspad で 8 node 分の flag が全部既知）、未知 flag と sha 不一致で exit 78、flag 照合が接頭辞で誤爆しないこと（`--palw-produc` は未知扱い）、`t12check.py` を隔離した Mac の node に当てて fp・genesis・registry が読めること。リモートでは何も実行していない。

## 9. 2026-09-24 決定と §7 の回答（オーケストレータ追記）

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
