# testnet-12 G14 17,400 release — 配備手順・fence 後 runbook・判断材料

2026-10-27/28 作成(lane DEPLOY-PREP、branch `claude/kit-17400`、base `claude/g14-rc-17400` @ 05ef4a741)。
**配備実績は `docs/design/palw/integration/g14-17000/release-17400-2026-10-28.md` に記録する。**
2026-10-28 の continuation で、最終 19 suite と re-arm gate 19 test が通過。期待 fingerprint は停止前の `.t-arm7.log` と今回の gate が一致した `73e67678…5bd5` に更新した。旧 0022568d… は以前の候補の値。公開先の切替はまだ実行していない。 live には読み取り(ssh の `systemctl show`・`/proc`・cgroup・launch script の閲覧、
TCP 接続確認)しかしていない。実行は Lead がユーザー承認の後に行う。

## 0. 前提と時刻

| 項目 | 値 |
|---|---|
| live release | `ddf5485d88df`、fp `ef379378…f15e`(= `UPGRADE_FROM_FP`) |
| 候補 | `claude/g14-rc-17400` @ 05ef4a741 → ACT2 が re-arm と記録 pin(R-REARM gate 緑)した commit を build する |
| 期待 `EXPECT_FP` | `73e67678a432effb4a982386d67383b95cf172babdde3e6f86c31cf3268d5bd5`(**re-arm 後の build の IDENTITY で確定させる**。違えば止まる) |
| fence | `G14_FLAG_DAY_DAA=17400`(`PALW_T12_G14_17000_FLAG_DAY_DAA`) |
| DAA 速度 | 10-27 20:14→21:14 CET で 16,699→16,720(≈21 DAA/h) |
| **完了期限 DAA ≈17,150** | ≈ **10-28 17:30〜18:00 CET**(17,400 は ≈ 10-29 05:30 CET)。build は 10-28 10:00 CET 以降 |

`fleet.env` 下書き: `scratchpad/kit17400/fleet.env`(git 管理外、`*.env`)。candidate の `fleet.env.example` に live kit の値
(genesis・premine・drill geneses・HB_ADDR)と **Phase H block(`ART_PH` 等)をそのまま** 足したもの。Phase H block を落とすと全 8k seat が
`--palw-class-artifact=<smollm2 .palwtir>` を失い `--palw-verify-class-manifest` が付く(意図しない ARGS 変更、FP-TIR が使う TIR class
f7e45711 の保持者が消える)。下書きと branch の kit で全 8 node の ARGS を生成し、live の launch script と比べた結果:
**b0 b1 b3 b5 b6 b7 は同一、b2 と b4 は `--addpeer=127.0.0.1:26341` の追加だけ**(b5 の復帰。意図した差)。

埋める値(re-arm 後の build の後): `REV`、`KASPAD_SHA256` `MISAKA_SHA256` `PALW_CLASS_SHA256`、`SEEDER_SHA256`(release の
`misaka-dnsseeder`。`KEEP` にしない)、`EXPECT_FP`。埋め済み: `UPGRADE_FROM_FP=ef379378…`、`G14_FLAG_DAY_DAA=17400`、
`KERNEL_MODEL_FILE=/root/palw-class/wide128-v1-7.palwkm`、`KERNEL_MODEL_SHA256=d071d3e2…3173`、
`EVIDENCE_PROVIDER_DIR=http://169.58.232.113:8088`。`STORAGE_CAP_BYTES` と `KERNEL_REGISTER_CLASS_FILE` は空(§9、§7)。

## 1. kit の変更(この branch)

| file | 変更 |
|---|---|
| `install-5104.sh` | **b5 を NODES に戻す**(10-26 縮退前と同じ spec、share 3,584・MemoryMax 9G、peers b2+b4+公開 3)。b2・b4 の peer に 26341 を戻す。MemoryMax は 9G のまま(DEC-RAM) |
| `move-nodes.sh` | `RETIRES=()`(b5 は move-host.sh で退役されておらず、`moved-b5` marker も無い。unit は enabled で稼働中) |
| `install-113.sh` | **b7 MemoryMax 9G → 12G**。**b6 は 17G のまま**(下記) |
| `lib.sh` | `STORAGE_CAP_BYTES`(`--storage-cap-bytes`、空 = off)、`KERNEL_REGISTER_CLASS_*`(wide128 登録、1 node だけ)、`NODE_EXTRA_ARGS_B<id>`(1 node の一時 flag) |
| `distribute-from-mac.sh` | `register-class <bin>`(ibm へ)、`95111`(95.111 へ kaspad と seeder) |
| `upgrade-95111.sh` | 新規。95.111 `kaspad-t12` の preflight / stage / swap / verify / rollback(§4) |
| `seeders/lib-seeders.sh` | seeder3 の「同居 public node」を `yes` に(95.111 は 0.0.0.0:26311 の node を持つようになった) |
| `fleet.env.example` | 上記の新しい knob(既定は空 = 何も変えない)。`G14_FLAG_DAY_DAA` は re-arm と同じ commit で埋まる(定数試験が pin) |

**b6 の MemoryMax について(判断が要る)**: 指示は「.113 の b6・b7 を 12 GiB」だが、b6 は今 17G で動いていて、10-27 21:11 の
cgroup は memory.current 11.4 GiB(anon 7.15 + file 4.3)、memory.peak 13.9 GiB。b6 の share は 8 GiB で、ledger の cgroup 項
(memory.max − memory.current − 1 GiB、page cache 込み)は 12G だと share を大きく下回り duty を断る。b6 は公開入口・heartbeat
miner・explorer の node。**kit は b6 を 17G に据え置いた**(DEC-RAM の記録も「2-seat host の seat を 12 GiB に」= 9→12 の引き上げと読める)。
文字どおり 12G にするなら `install-113.sh` の b6 行の 17 を 12 に変える 1 文字の変更。

## 2. 配備の順序(Lead、承認後)

| 順 | どこで | 何を | 次へ進む条件 |
|---|---|---|---|
| 0 | Mac | ACT2 re-arm commit を確認(R-REARM gate 緑、`G14_FLAG_DAY_DAA=17400` が example と core で一致) | — |
| 1 | Mac | `CACHE= ./build-release-local.sh <re-arm commit>`(既定 `.cache`) | `.cache/<rev12>/IDENTITY` の `EXPECT_FP` が 73e67678…5bd5(違えば **中止**して ACT2 に確認) |
| 2 | Mac | `fleet.env` を下書きから作り、REV・4 sha・EXPECT_FP を IDENTITY/SHA256SUMS から貼る | `distribute-from-mac.sh binaries` の identity_check が通る |
| 3 | Mac | `./distribute-from-mac.sh kit` → `binaries` → `kernel-model <dir>/wide128-v1-7.palwkm`(Mac で `gen-wide128-palwkm` を実行、sha d071d3e2…) → `95111` | 各 host で sha256sum 一致 |
| 4 | 3 host | `./install-<host>.sh stage`(kernel model を各 node の `palw-kernel-models/` に置く) | 全 launch script の `--check` OK |
| 5 | 3 host | `DRY_RUN=1 UPGRADE_ARGS_CHANGE_OK=1 ./install-<host>.sh upgrade` | 最後まで通り、ARGS diff が **5.104 の b2・b4 の `--addpeer=127.0.0.1:26341` だけ**(他 host は diff なし)。それ以外の diff は中止 |
| 6 | ibm【確認】 | `./install-ibm.sh upgrade`(pinner → b0 → b1 → b3) | ibm `check` + Mac `CHECK_REGISTRY=1 ./check-fleet.sh` を 2〜3 分あけて 2 回: 新 fp・synced・DAA 前進・8k `readySeatsNow ≥ 7` |
| 7 | .113【確認】 | `./install-113.sh upgrade`(pinner → b6 → b7。b7 の MemoryMax 12G はここで入る) | 同上 + misakascan の表示が進む。`systemctl show -p MemoryMax misaka-t12-seat7` = 12884901888 |
| 8 | seeders【確認】 | **.113 の直後に** §5 の手順で 4 台を release の seeder へ | `40-verify.sh` 4 台 PASS(両 anchor を新 schedule で検証) |
| 9 | 5.104【確認】 | `UPGRADE_ARGS_CHANGE_OK=1 ./install-5104.sh upgrade`(pinner → b2 → b4 → b5、30 s 間隔) | `CHECK_REGISTRY=1 ./check-fleet.sh` で 8 node OK |
| 10 | 95.111【確認】 | §4 の `upgrade-95111.sh` | verify で SHA OK・FP OK・DAA 前進 |
| 11 | Mac | `CHECK_REGISTRY=1 CHECK_G14=1 ./check-fleet.sh`、`seeders/40-verify.sh`、`CONFIRM=yes seeders/60-join-check.sh <release kaspad>` | **DAA ≈17,150 までに 0〜11 が全部済む** |

順 8 を .113 の直後に置く理由: seeder は anchor の fence schedule id を **完全一致** で比べる(`misaka/apps/dnsseeder/src/main.rs:191`)。
新 binary は 17,400 を予定に持つので、anchor が新 binary になった瞬間から旧 seeder(1174b965)はその anchor を広告しない。ibm 更新後は
.113 だけ、.113 更新後は **0 件**(空応答)になる。17,400 を待たずに起きる。.113 と seeder の間は短くする(数分)。

### 中止条件(その場で止め、残りの host は旧 release のまま)
- IDENTITY の EXPECT_FP が期待値と違う / `distribute binaries` の identity_check 失敗。
- DRY_RUN の ARGS diff が上の想定以外。
- upgrade の gate 失敗(fp 違い、genesis 違い、`Deleting databases`、DAA が停止前より小さい、600 s で synced にならない)。
- 8k `readySeatsNow < 7`、heartbeat が止まる(DAA が 5 分進まない)。
- 期限 DAA 17,150 までに全 host が終わらない見込み → 下の「間に合わない場合」。

### 戻し
- **17,400 前**: 問題の host から `./install-<host>.sh upgrade-rollback`(REV は新 REV のまま、複数なら 5.104 → .113 → ibm)。
  95.111 は `CONFIRM=yes ./upgrade-95111.sh rollback`。seeder は `CONFIRM=yes seeders/50-rollback.sh <name> <TS>`(**anchor が旧 binary
  に戻った後で**。新 seeder は旧 anchor を広告しない)。
- **17,400 後**: 旧 binary に戻すと旧規則で新 block を拒否し得る(fence を跨ぐ drill は省略済み、戻しは未試験)。戻しではなく修正版の
  前進配備。旧 binary のまま・fp が他と違う node は **跨ぐ前に `systemctl stop`**(新 binary にしてから空 datadir で resync)。
- **間に合わない場合**: 17,150 時点で未更新の host が残るなら、その host の node を 17,400 前に止める(上と同じ)か、Lead が fence 延期を判断。
  公開入口(.113 b6)と 8k producer(ibm b0)が両方新 binary であることが最低条件。

## 3. b5 の kit 復帰(O-B5 / DEC-B5)

10-27 に手で作った `/root/t12-rel/ddf5485d88df/launch/b5.sh` の ARGS は、この kit が生成する b5.sh と **完全に同じ**(上の比較)。
drop-in `zz-t12-regenesis.conf` は kit 形式(header `# deploy-t12 drop-in`、ExecStart = `$REL_ROOT/<rev>/launch/b5.sh`)なので、
`upgrade` の事前確認を通り、`.before` を `upgrade-<REV>/misaka-t12-seat5.dropin.before` に保存して入れ替える。header の
"release 247ade1f90b6" は表示だけで判定には使われない。退役 marker(`$STATE_DIR/moved-b5`)は無く、unit は enabled。

## 4. 95.111 `kaspad-t12`(O-SEED3、kit 外)

実測(10-27 21:11 CET、読み取り): unit `/etc/systemd/system/kaspad-t12.service`、ExecStart `/root/t12-sync/node.sh`、binary
`/root/t12-rel/247ade1f90b6/bin/kaspad`(sha c57c8c13…)、appdir `/root/.t12-247ade1f90b6`(7.6 GB)、0.0.0.0:26311(外から到達可)、
rpc 26312/26313/26314、evm 8545、`--ram-scale=0.3 --min-disk-free-percent=5`、bond なし。MemoryMax 9G は 2 か所
(`system.control/…/50-MemoryMax.conf` と `kaspad-t12.service.d/zz-memory.conf`)、MemorySwapMax 0。cgroup: anon 7.68 GB・file 0.40 GB、
`memory.events max=18,033`(上限で reclaim を繰り返している)、oom_kill 0。host 11.96 GB・6 core・空き 106 GB。同居: t10
`misaka-validator-b`、ollama、docker(highroller-web・roulette-webgl・pm2)、nginx、`palw-liveness-watch`(記録のみ、再起動しない)、
`misaka-dnsseeder-t12`(seeder3、`--node-wrpc-borsh 127.0.0.1:26313`)。**これらには触れない。**

**args の差は binary の path だけ**(`/root/t12-rel/<REV>/bin/kaspad`)。appdir 名は旧 rev のまま(同じ chain。変えると resync)。

```
./distribute-from-mac.sh 95111                 # $INCOMING/$REV に kaspad(+ seeder)
./upgrade-95111.sh preflight                   # 読み取り
CONFIRM=yes ./upgrade-95111.sh stage           # /root/t12-rel/$REV/bin/kaspad(sha 照合)、node.sh.new-$REV と diff 表示
CONFIRM=yes ./upgrade-95111.sh swap            # node.sh.bak-247ade1f90b6 に退避 → stop → 入替 → start → 120 s 後 verify
./upgrade-95111.sh verify                      # exe sha・FP 行・DAA・oom_kill・p2p 接続数
CONFIRM=yes ./upgrade-95111.sh rollback        # 17,400 前のみ
```

MemoryMax は既定で 9G のまま。anon 7.7 GB で上限に張り付いているので、`MEMMAX_GIB=10`(swap 時に両方の drop-in を書き換え、rollback で
戻す)を提案する。ただし host は 12 GB で ollama・docker と共有。決めるのはユーザー/Lead。swap 中(〜2 分)は seeder3 の backing node が
無いが、`--anchors-only` は co-located node を best-effort でしか使わないので応答は続く。

## 5. dnsseeder 4 台の入替(R-DEPLOY)

`SEEDER_SHA256` = release の `.cache/<rev12>/misaka-dnsseeder` の sha。手順は SEEDERS.md §5 の scripts:

```
./seeders/00-preflight.sh                                   # 読み取り
CONFIRM=yes ./seeders/20-stage.sh c5104   .cache/<rev12>/misaka-dnsseeder   # smoke あり(同居 node なし)
CONFIRM=yes ./seeders/20-stage.sh seeder3 …   ; … ibm … ; … seeder1 …        # 同居 public node あり → smoke なし
# (§2 順 7 の .113 upgrade の直後)
CONFIRM=yes ./seeders/30-swap.sh c5104   --binary $SEEDER_SHA256
CONFIRM=yes ./seeders/30-swap.sh seeder3 --binary $SEEDER_SHA256
CONFIRM=yes ./seeders/30-swap.sh ibm     --binary $SEEDER_SHA256
CONFIRM=yes ./seeders/30-swap.sh seeder1 --binary $SEEDER_SHA256   # seeder1 と seeder3 を同時に再起動しない
./seeders/40-verify.sh                                         # 4 台 PASS(UDP 応答が空でない)
```

### seeder 冗長化の提案(判断材料)
今: 4 台とも `--anchors-only`、anchor 2(.113:26311、ibm:26311)、効いている DNS 名は seeder1・seeder3 の 2 つ(seeder2・4 は管理外 IP に委任)。

| 案 | 効果 | 費用・リスク |
|---|---|---|
| A. **95.111 を 3 本目の anchor に**(`30-swap.sh <name> --anchors 169.58.232.113,169.58.39.220,95.111.236.186`) | 既定 port の入口が 3 か所に。95.111:26311 は外から開いている(5.104 から確認)。§4 の upgrade 後なら schedule 一致 | 95.111 は bond なしの同期 node で、RAM が上限に近い(§4) |
| B. seeder2・seeder4 の NS glue を 5.104 / ibm へ(レジストラ操作) | 動いているのに使われていない seeder 2 台が有効に(DNS 名 2→4) | ユーザーの操作、反映最大 1 h。5.104 は inbound を落とすので 5.104 の node は配らない(seeder だけ動く) |
| C. 1〜2 台で `--anchors-only` を外す | 近接 node の peer 一覧から handshake 検証済みの peer も配る(2 件固定でなくなる) | 外部 node を広告する。seeder は identity と schedule の完全一致を確かめるので別 chain は入らない |

推奨: 17,400 では入替だけ(変数を増やさない)。fence 通過後に A → B の順。C は A・B の後に様子を見て。

## 6. 各 step の確認コマンド

```
CHECK_REGISTRY=1 ./check-fleet.sh                          # 8 node: fp / genesis / synced / peers / 8k readySeatsNow
CHECK_G14=1 ./check-fleet.sh                               # G14 counters(kernel Finals、convicted/DA forfeited after Final)+ G14 log 数
./install-<host>.sh check                                  # host 上: cgroup・journal の要注意行
journalctl -u <unit> --since -10min -o cat | grep -E 'Consensus params fingerprint|kernel verifier: holds the model'
```

## 7. fence 後 runbook(17,400 以降)

### 7.1 wide128 の登録(R4、attested root `44c62870…960e`、class id `6a85c5…ec55`)
1. Mac: `cargo run --release -p misaka-palw-kernel --example gen-wide128-palwkm -- <dir>` → `<dir>/wide128-v1-7.register-class.bin`。
   fleet.env に `KERNEL_REGISTER_CLASS_FILE=/root/palw-class/wide128-v1-7.register-class.bin`、`KERNEL_REGISTER_CLASS_SHA256=<sha>`、
   `KERNEL_REGISTER_CLASS_DRY_RUN=1`(既定 node = card 1 = ibm b1)。`./distribute-from-mac.sh kit` → `register-class <bin>`。
2. ibm: `./install-ibm.sh stage`(b1.sh だけが変わる)→ `systemctl restart misaka-t12-node1`。
   log: `journalctl -u misaka-t12-node1 -o cat | grep -i 'register'` で class id 6a85c5…ec55・root 44c62870…・bond が出ること(投稿しない)。
3. `KERNEL_REGISTER_CLASS_DRY_RUN=0` → kit 配布 → stage → `systemctl restart misaka-t12-node1`(class が route に載るまで再投稿し、載ったら止まる)。
4. 確認: RPC `getPalwKernelRows`(class が載る)、全 node の log に `kernel verifier: holds the model … commitment root 44c62870…`。
5. `KERNEL_REGISTER_CLASS_FILE=` に戻し → stage → b1 restart(flag を外す)。
(`upgrade` は同じ REV で動いている node を再起動しないので、1 node の flag 変更は stage + その unit の restart で行う。)

### 7.2 最初の Panel=1 job → Final → Gate C → 支払い
1. poster(既定 b1): `NODE_EXTRA_ARGS_B1="--palw-kernel-model=/root/palw-class/wide128-v1-7.palwkm --palw-kernel-post-job=<tok,tok,..>:<max_new_tokens>"`
   で stage → restart(fee は b1 の `--palw-fee-outpoint`、escrow は poster 払い、claim 担保 1,080 MSK)。
2. producer: `--palw-kernel-producer`(kernel model を持つ node。必要なら同じく `NODE_EXTRA_ARGS_B<id>`)。
3. 観察: `getPalwKernelRows` / `getPalwKernelClaim {claimId}`(seal → reveal → Panel=1 verdict)、`getPalwKernelFinals`
   (`standing = Standing`)、`CHECK_G14=1 ./check-fleet.sh`。Gate C(`SecurityCertified`)の後に escrow から 1 回だけ支払い。
   受取の UTXO を wallet CLI で spend して終わり。kernel claim は B = F = 0(合意重みなし)。
4. 終わったら `NODE_EXTRA_ARGS_B*` を空に戻し stage + restart。
5. 止める条件: `ConvictedAfterFinal` / `DaForfeitedAfterFinal` が正直な job で出る、node の panic、fp の不一致。

### 7.3 FP on TIR class(lane C `claude/fp-tir` cfa4a6c70、R-FPTIR)
lane C の runbook(`docs/evidence/palw/e2e-real-node/FP-TIR/README.md` §4)をそのまま使う。**ただし B1〜B5 の blocker が残る**:
B1 `palw_class_semantics_v1` が t12 のどの list にも無い → 17,400 後は public FP job がすべて `G14_ROUTE_ABSENT`。B2 semantics 宣言の
operator tool が無い。B3 f7e45711 の PALWART3 binding 未確認。B4 Probation(0/10)と R-SEAT。B5 certified family。
この release では **FP は止まる前提**(waiver に記載)で、B1 の扱い(list に足す = 指紋が動く / FP off を waiver に記録)は Lead 判断。
B1〜B3 が解ければ: 17,300 までに PALWART3 binding → semantics 宣言 → Probation 完了、17,400 後に
`rail … --class-id f7e45711… --tfp1 require … --submit --watch`、seat の fp-worker、log の
`G14_ROUTE_ABSENT|G14_JOB_VERSION_UNJUDGEABLE|TIR_UNBOUND_CONTAINER|BondBudgetExhausted` を監視、`CHECK_REGISTRY=1 ./check-fleet.sh`。

## 8. Panel 容量(O-LIVE-1、読み取りのみ、10-27 21:11〜21:13 CET)

| host | vCPU | load 1/5/15 | CPU PSI some avg60/300 | MemAvailable | seat(lifetime CPU、cgroup peak) | replay slot |
|---|---|---|---|---|---|---|
| ibm | 8 | 17.9 / 12.4 / 8.5 | 60 % / 42 % | 9.96 GB | b0 2.6 core 7.1 GiB、b1 2.0 core 7.9 GiB、b3 2.1 core 7.5 GiB | 既定 2(flag なし) |
| .113 | 8 | 11.3 / 7.9 / 4.9 | 25 % / 18 % | 15.5 GB | b6 1.9 core 13.9 GiB、b7 1.0 core 7.7 GiB(+ explorer・postgres) | 既定 2 |
| 5.104 | 8 | 4.7 / 9.5 / 9.2 | 31 % / 33 % | 17.2 GB | b2 2.1、b4 2.2、b5 2.3 core、7.6〜8.9 GiB | `--palw-seat-replay-slots=1` |

訂正: 「全 seat が slot 1」ではない。slot 1 は 5.104 の 3 seat だけで、ibm・.113 は既定 2(`PalwSeatReplaysV1::IN_FLIGHT`、
load/CPU > 3.0 で自動的に 1)。ただし 8k replay は share 3,584 MiB で 1 本しか全幅で走らないので、2 本目は幅が狭まる(W=1 で 2.5 倍遅い)。

**slot の提案**
- 5.104: **1 のまま**。CPU は 3 seat で ≈6.6/8 core、PSI 33 %。2 本目は share 不足で狭い幅になり、10-02 の実測で遅くなった。
- ibm: **変えない**。CPU PSI 42〜60 % で飽和。
- .113 b7: **余地あり**。b7 は 1.0 core、host に ≈3 core と 15 GB の余裕。MemoryMax 12G(今回)の上で share を 3,584 → 7,168 MiB に
  すると 2 本が全幅で走る(floor 7,168 + 1,716 + 1,024 = 9,908 MiB < 12,288)。share の変更は ARGS 変更なので **17,400 では入れず**、
  fence 後の node-only 更新で。見込み +4〜6 replay/h(≈+10 %)。

**seat 数の見積り**: 必要 ≈75 replay/h(有料 15 claim/h、Coverage door が実質 5-of-5)に対し、今の容量は ≈54〜58 replay/h(8 seat、
≈7 replay/h/seat)。75 / 7 ≈ **11 seat**、余裕 1.3 倍で **14 seat**。1 seat ≈ 2〜2.5 core・8 GiB(cgroup peak)・ディスク 15〜30 GB。
5.104 の経験から 8 vCPU / 24 GB の host には 2 seat が上限。→ **+3 seat(11)なら 8 vCPU host 2 台、+6 seat(14)なら 3 台**。
b7 の share 引き上げ(+0.5〜1 seat 相当)を先に入れると、+2 台で 11 seat 相当を超える。hardware の調達はユーザー判断(DEC-INFLOW)。

## 9. storage cap(`--storage-cap-bytes`、F7)

- 今の appdir: b0 11 G、b1 11 G、b3 9.8 G、b6 11 G、b7 10 G、b2/b4/b5 9.3〜9.4 G。≈0.66 MB/DAA で増え、最初の prune(≈DAA 22,100、
  ≈11-08)の時点で ≈15 GB。E2E-OPS の見積りは定常 ≈14 GB、WAL 込みの山 ≈28 GB。
- 空き: ibm 69 GB(3 node)、5.104 81 GB(使っていない `.t12r-b3`・`.t12r-b7` 各 4.4 GB が残る。消すかは別判断)、.113 98 GB、95.111 106 GB。
- **推奨値 30,000,000,000 byte / node**(binary の推奨と同じ)。ibm は 3 × 30 GB = 90 GB が上限で、今の使用 + 空き ≈ 100 GB に収まる。
- **17,400 では入れない(既定 空)**。理由: cap に近づくと node は書き込みの受け入れを止める(consensus node としては停止に近い)、
  t12 では一度も使っていない、E2E-OPS の full-scale F7 run が 10:00 CET 以降。F7 run で山が 30 GB 未満と確認できたら、fence 後の
  node-only 更新で seat から入れる(`STORAGE_CAP_BYTES=30000000000`、`UPGRADE_ARGS_CHANGE_OK=1`)。`--storage-cap-bytes` は
  `--storage-gc-pruned-evm-indexes` を含むので、EVM RPC を出す b6(explorer `/evm`)は最後に、prune 済み block の EVM 索引が消えることを
  explorer 側で確かめてから。
