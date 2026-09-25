# testnet-12 プレドリル（c8652a97 + drill salt + drill main wallet）— チェックリスト

対象は **drill build 005e5c5f**（branch `drill/t12-pre-c8652a97`、bundle `ship-pre2.bundle` sha256 `33ef9c68…`）。中身は launch candidate
**c8652a97**（panel-room review f8c91f19/92a9658d/0533e1de、licence-stall 修正 a4dfe903/d94d3a1b、運用者の 100M community 行 5bf72b46
＝公開 genesis a27f8f44…）に、drill 専用 commit を 1 つ足したもの: chain の identity を塩漬けし、drill chain の main wallet を
**TESTNET_MAIN_ADDRESS**（公開テスト seed `tests::TESTNET_MAIN_SEED` から再生成できる鍵）にする。**出荷しない**。証拠は
「c8652a97 のルールが、公開 t12 と outpoint も genesis も共有しない chain の上でどう動くか」についてのもの。

| 何 | drill chain | 公開 t12 ほか（drill chain に**絶対に出てはいけない**） |
|---|---|---|
| premine salt | `misaka-palw-t12/premine/v2/2026-09-24/DRILL-PRE-c8652a97` | `…/2026-09-24`（公開）、`…/DRILL-PRE-e93be0f2`（1 回目の drill） |
| premine txid | `269a354e…1ac0918d`（premine.rs の式で再計算して一致を確認） | `5e0d5f1b…e55e2669`（公開）、`65093cb6…8e8ff07b`（1 回目の drill）、旧 sentinel `6d697361…00` |
| genesis | `32a665d6…a994705f`（timestamp 1788220860000、utxo commitment `d50a03ca…`） | **`a27f8f44…a8ca1f23`（公開、c8652a97）**、`d73dbf44…f230fe18`、`f6cc9576…3a963a30`、`fb1074b0…e47469aa`、`a8cabac4…590777dd`、`1eaa6c0f…b89d6d7c`（1 回目の drill）— 6 本とも全 hash |
| main wallet（premine `:40`） | **TESTNET_MAIN_ADDRESS** `misakatest:qtpflz03…a2yqlxuz`（公開テスト鍵。`269a354e…:40` と bond collateral `:0..7` を持つ） | 公開 t12 は運用者の `PALW_PUBLIC_MAIN_ADDRESS` `misakatest:qf7hzj76…0tawqn8x`（`5e0d5f1b…:40`）。キットはこの鍵を持たず、これに解決する鍵を拒否 |
| params fingerprint | C1 が全 node から RPC（`getPalwNodeStatus.consensusParamsId`）で取得。genesis.hash を hash するので公開のどれとも一致し得ない | `fb8f378d` `c746f07c` `30848c6b` `bcfbf2a3` `f66bf139` `88a9aee8`（c8652a97 公開 build の値は未記録 → `PUBLIC_FP=` で追加） |

このキットで実行済みなのは: `bash -n`・`py_compile`、Mac 上での build-ship.sh の **source marker 48 個と禁止 genesis bytes 6 本の空打ち**
（005e5c5f の worktree で全数一致）、`drill.sh mainkey` の import 経路を Mac の misaka CLI で空打ち（seed → `key import --hex-stdin` →
address = TESTNET_MAIN_ADDRESS を確認）、C14 の判定式を偽 RPC で単体確認、そして **A1 の build を 5.104 で開始**（`build.log`）。
binary marker 18 個は build 後に build-ship.sh 自身が確かめる。node は 1 台も起動していない。

---

## PLAN — 5.104 での実行順（合図が出てから）

**前提（合図の条件）**: 別 session が `misaka-t12p-0..7`・scan・tunnel・daa-obs を止めたこと。`misaka-t12f-b2..b5`（26511–26543）、
pid 2990090（t11 kaspad :36311）、:26313 の python poller、`/root/misakas*`、**`/root/perm-drill`（読み取りも含めて一切）**、
`/root/t12-private`（鍵の読み取り以外）、root 自身の `~/.misaka` には触れない。ドリルは 127.0.0.1 の 28601–28693 だけを使う。
misaka-ibm（169.58.39.220）と 169.58.232.113 には接続しない。

**壁時計**: この build は clock floor（`palw_clock_floor`、H3/H5）を DAA 0 から武装しているので、clock は 120 s の slot に最大 1 回しか
進まない。**主な見積もりは 2.05 分/DAA（≈ 123 s）**（右の 3.5 分列は floor 以前の live t12 実測に基づく悲観値）。6 h の窓は **約 175 DAA**。
C1 が n0 の `advanced the clock to DAA N` 行から実測値を `run/pace.s` に書き、以後の待ちは ETA をその値でログに出す（`./drill.sh pace` で
いつでも再測定）。タイムアウトは従来どおり `daa_wall_s N = N × 360 s + 900 s`（6 分/DAA）で、30 分 DAA が動かなければ停止する。

**held な 8k 行（c8652a97 の新しい前提）**: 8k 行 `ebf44d0a…` は held class（`class_is_held_v1`、window 3 spans、`max_inflight_claims` 5）で、
各 claim は **Final まで**予算に載り続ける（licence では解放されない、ADR-0152 T-2(b)）。最初の 8k Final（≈ 8k licence + 120 DAA ≈ DAA 190–205）
までに in-flight の 8k claim は最大 5 本で、それ以後 n2 は `holding: class … has no panel room left in the network's verification budget` で
保持する。C5・C10・C14 はこれを前提に書き直した（下の表）。

### A. 準備（node はまだ起動しない）

```bash
# A0  キットを配置（5.104）— 済（本 retarget で再配置、sha256 照合）
cd /root/drill-t12/kit && install -m 0755 lib.sh drill.sh chainstate.py reorgcheck.py build-ship.sh /root/drill-t12/ \
  && install -m 0644 CHECKLIST.md /root/drill-t12/
cd /root/drill-t12
# A1  build（35–50 分、nice 10、-j4、CARGO_INCREMENTAL=0）。SHIP_REV=005e5c5f…、SRC_FROM=ship-pre2.bundle が既定 — 開始済み
nohup ./build-ship.sh > build-ship.out 2>&1 < /dev/null &      # 完了の印は build.log の BUILD-DONE
# A2  8k artifact を Mac から置く（A1 と並行でよい。所有者が承認した経路。/root/perm-drill は使わない）
#     Mac 側で:
#       scp -o IdentitiesOnly=yes -i ~/.ssh/claude_key \
#         /private/tmp/claude-501/-Users-wata-Downloads-MISAKA-testnet/24499bec-23cb-41cf-b06f-867a448df39b/scratchpad/deploy-t12/.cache/art/qwen25-1.5b-a16-8k.palwart{,.palwmanifest} \
#         root@5.104.81.23:/root/drill-t12/art/        # 先に 5.104 で: mkdir -p /root/drill-t12/art
./drill.sh verify-art        # 1,799,359,436 bytes、sha256 b73600cf…/c3cc66bc…、sidecar が class ebf44d0a… を名指す
# A3  drill 専用 wallet 12 個（pay-0..6, hb-0, hb-3, x1..x3）＋ drill main wallet の import（keygen が最後に mainkey を呼ぶ）
./drill.sh keygen            # keygen.sha256 に 12 行、mainkey.sha256 に 1 行、keys/main.addr = TESTNET_MAIN_ADDRESS
```

### B. 窓（~6 h ≈ 175 DAA）

```bash
./drill.sh preflight                               # C0
nohup ./drill.sh observe > run/observe.out 2>&1 &  #     常駐の読み取り専用サンプラ（メモリ非常停止つき）
./drill.sh up                                      # C1（n3 → n0 → n1 n2 n4 n5。pace を実測。main wallet が 269a354e…:40 を持つことも記録）
./drill.sh alarm &  ./drill.sh b1                  # C2 と（C3a → F0 → C3b → C4 の仕込み）を並行（b1 は n8 を一時的に使う）
wait
./drill.sh hold7                                   # C7 + C8
./drill.sh restart  &  ROOM_SAMPLES=20 ./drill.sh room   # C5 と C14 を並行
wait
./drill.sh licence                                 # C13（1 回目）— 期待値 PASS（licence-stall 修正入り）
PART_DAA=4 ./drill.sh reorg span                   # C9-span
./drill.sh ibd                                     # C6
./drill.sh cheapcheck                              # C4-bind、C4-cheap-bond（x1 宣言後の floor 抽選が 10 本未満なら NOT-REACHED、終盤に再実行）
./drill.sh maturity                                # C16（窓内は NOT-REACHED が正常）
GATE_SHORT=1 ./drill.sh gate                       # C12-short
./drill.sh licence                                 # C13（2 回目）
./drill.sh cheapcheck                              # C4-cheap-bond の本判定（抽選 10 本以上）
./drill.sh second                                  # C15（この roster では NOT-REACHED が期待値）
./drill.sh pace                                    # DAA ≥ ~150 かつ窓が 45 分以上残るなら次を実行、でなければソーク S1 へ
FINAL_BOND=1 LEAD=3 ./drill.sh reorg final         # C9-final（最初の floor Final ≈ DAA 160–165。窓の最後に届くかもしれない）
# 窓終了。silence（C10）は窓内では走らせない — held な 8k 行は最初の 8k Final まで新しい claim を bind しない（ソーク S3）。
# ソークを続けない場合: ./drill.sh down
```

### F0: drill main wallet から x1..x3 への資金（キットの手順。b1 が C3a の直後に自動で実行）

drill chain の main wallet は **TESTNET_MAIN_ADDRESS**（drill 専用 commit が t12 の main wallet をこれに変えた）。鍵は公開テスト seed
`tests::TESTNET_MAIN_SEED`（premine.rs、`testnet_main_key_is_reproducible`: ML-DSA-87 seed = BLAKE2b-256(seed)）から再生成できる。
`drill.sh mainkey`（A3）が build 済み tree（`/root/drill-t12/src`、005e5c5f）から seed を読み、キットの写しと一致を確認し、64 桁の hex を
**パイプで** drill の CLI `misaka key import --out keys/main.key --hex-stdin` に渡す（argv にも環境変数にも載せない。ファイルは CLI が書く
0600 の key file だけ）。address が TESTNET_MAIN_ADDRESS でなければ消して停止。sha256 を `mainkey.sha256` に記録。

**この公開鍵で署名してよいのは premine txid が salt 済みだから**、そしてそれが証明されたときだけ:

```bash
./drill.sh fund     # = F0。b1 が C3a PASS の直後に呼ぶ。単独で走らせてもよい（x1 は C3a PASS まで資金を入れない）
```

1. **chain**（`assert_drill_chain`）: `bin/REV` = 005e5c5f…、n0 が drill の kaspad として稼働、`chainstate genesis` 合格、かつ n0 が
   genesis `32a665d6…` を **DAA 0 で答える**（pruned-past は不可）、bond が `269a354e…` 上、禁止 genesis 6 本と禁止 premine 3 本の bond なし。
2. **鍵**（`assert_drill_key` → `assert_main_key_file`）: `keys/main.key` が 0600、sha256 が `mainkey.sha256` と一致、address = TESTNET_MAIN_ADDRESS、
   カード address でも運用者の公開 main wallet でもない。
3. **お金**: main wallet の output が `269a354e…:40`（初回は必須）と drill chain 上の change だけで、公開・sentinel・初回 drill の
   premine txid 上に 1 本もない（`ev/F0-fund/main-wallet.txt`）。
4. **送金**（1 本ずつ、確定してから次）: **x1 13,001 MSK**（13,000 MSK の producer-floor bond ＝ C3b・C4-cheap-bond）、
   **x2 135,001 MSK**（130,000 MSK の seat floor を超える control bond。`CONTROL=0` で省略）、**x3 20 MSK**（C11-fee の 5 回の支払い）。
   txid は `ev/F0-fund/sends.tsv`。既に bond 済み／資金済みの wallet には送らない（再実行は安全）。

同じ `assert_drill_chain` は**すべての送金の直前**にも走る: `fund`（F0）、`register_fresh`（x1/x2 の bond 登録）、`reg_expect`（C3a の
registrar）、`bond capability --declare`（C4）、`maturity`（C16）、`route`（C11-fee）。カード鍵は送金に使わない（`assert_drill_key` が拒否）。

### PLAN の表（所要は 2.05 分/DAA 基準、括弧内は 3.5 分/DAA）

| # | コマンド | DAA（genesis=0） | 所要 | PASS の条件 |
|---|---|---|---|---|
| A1 | `build-ship.sh` | — | 35–50 分 | `BUILD-DONE`。他の cargo/rustc が動いていれば開始しない。source marker 48 個（t12 の修正・drill salt・drill genesis/utxo commitment/timestamp・`return TESTNET_MAIN_ADDRESS;`・TESTNET_MAIN_ADDRESS/TESTNET_MAIN_SEED・100M 行・`key import --hex-stdin`・rate room（`panel_room_by_rate_v1`・`palw_panel_capacity_by_rate_v1`・`palw_panel_owed_v1`・`palw_panel_room_read_v1`）・held 上限（`class_is_held_v1` の gate、`ClassInflightCapped` の文言）・licence-stall（`palw_select_optimistic_licence_v2`・`RECEIPTS_V3_MAX_CLAIMS`・`seat_duty_panel_key_v1`）・producer の room 文言、ほか従来の前提）、genesis.rs に禁止 genesis bytes 6 本なし、binary marker 18 個 |
| A2 | scp + `verify-art` | — | 5–30 分（回線次第）+ 1 分 | 2 ファイルが `/root/drill-t12/art/`（perm-drill に解決しない）、1,799,359,436 bytes、sha256 一致、sidecar が `ebf44d0a…` を名指す |
| A3 | `keygen`（+ `mainkey`） | — | < 1 分 | 12 wallet、address がカード 8 枚（= payout）とも公開 main とも TESTNET_MAIN_ADDRESS とも互いに素、重複なし、`keygen.sha256` に 12 行。`keys/main.key` の address = TESTNET_MAIN_ADDRESS、`mainkey.sha256` に 1 行 |
| C0 | `preflight` | — | 3–8 分（8k の root walk） | `bin/REV = 005e5c5f…`、`src` が 005e5c5f、sha256 一致、flag 13 個、kaspad に drill salt・両 binary に禁止 genesis bytes 6 本なし、KASPAD_/MISAKA_ の残りなし・`HOME=/root/drill-t12/home`、root の endpoints.json の sha/mtime を記録、カード鍵 8 本（存在のみ）、drill wallet 12 個が `assert_drill_key` を、main.key が `assert_main_key_file` を通る、verify-art と `palw-class manifest --check` 合格、ポート空き、t12p 停止、MemAvailable ≥ 17 GiB、空き ≥ 40 GB |
| C1 | `up` | 0→3 | 10–15 分（15–25） | n3・n0 起動直後に `chainstate genesis` 合格、**hub n0/n3 の P2P port が LISTEN**、n1–n5 も genesis 合格、6 台の sink 一致、DAA +3、**RPC の fingerprint が 6 台で 1 種類かつ公開 denylist 外**、カード 0–7 が drill premine 上で `registered True, owned_by_supplied_key True`、公開・sentinel・初回 drill premine に bond なし、**drill main wallet（TESTNET_MAIN_ADDRESS）が `269a354e…:40` を持つ**、heartbeat の granted/holding/NOT-granted を記録、pace を記録、DNS は `Bootstrap` |
| C2 | `alarm` | 0→~22 | 35–50 分（55–80） | `[palw-lane-watch] … no PALW work block in the newest` の ERROR、RPC `laneAlarm`、`gate --expect-heartbeat-only` が拒否。floor producer 起動後に `PALW work is back` と `laneAlarm` 空 |
| C3a | `b1`（並行） | ~5–20 | 5–15 分 | 0.1 MSK と 13,000 MSK − 1 sompi が `--palw-bond-collateral … is below this chain's floor of 1300000000000 sompi` で**資金を見る前に**拒否、x1 が無資金のうちに floor ちょうどは `no confirmed UTXO to spend` で止まる、x1 の key モード bond status が `bonds_registered_to_this_key == []` かつ `bonds_unanswered == 0` |
| **F0** | `b1` が続けて実行（`fund`） | C3a の直後 | 6–15 分（send 3 本 × 確定待ち） | 上の F0 の 1–4: n0 が drill genesis を DAA 0 で答え、main.key が TESTNET_MAIN_ADDRESS、main wallet の output が drill premine 由来だけ、x1 13,001 / x2 135,001 / x3 20 MSK が 1 本ずつ node に受理され確定（`ev/F0-fund/{sends.tsv,sends.log,main-wallet.txt,*-after.json}`） |
| C3b | `b1`（F0 の後） | ~15–40 | 10–40 分 | 新規鍵 x1 が F0 の資金で 13,000 MSK ちょうどの bond を drill build の builder で登録、`BondRegistered` が carried（dropped なし）、bond status に x1 の bond。続けて x2（135,000 MSK）も登録、x1 だけ floor を宣言 |
| C7 | `hold7` | 30（grace）→~45 | genesis から ~1.5 h（2.7 h） | n6 なしで 8k 行が Prefetching、n2 の `holding: the model registry holds class ebf44d0a…: Prefetching …`、facts `notReadyReason` = `the model registry admits no new claim of this class now [...]`、`mining status` に `E-MODEL-NOT-ADMITTING`、保持中の生産 0。n6 起動後 readySeatsNow ≥ 7 → **variant が** Probation/ActiveLimited/Active → n2 が 8k block を生産 |
| C8 | 同上 | 同上 | 同上 | 7/7 seat で proof ≥ 1、**各 seat の最新の 8k readiness 事象が proof**（それ以前の一時的な `no proof` は記録のみ）、最後の proof 以後に `no proof`/`no proofs — this bond` なし、`memoryShareBytes = 3758096384`、`memoryReservedBytes ≤ share` |
| C5 | `restart` | ~65–85 | genesis から ~2.3–2.9 h（4.1–5 h） | 最新の bound 8k claim が、n2 と抽選 seat の再起動後に `receipt_licensed`/`final`、**n2 が戻ったこと**: 再び生産する、**または** held 8k の in-flight 上限（5、Final まで）で `holding: … has no panel room left …`（/`… against the registry's cap of …`）を出す（`n2-back.log`）、その claim の `ProducerDefaulted` なし |
| C14 | `room`（並行） | 8k 受け入れ中 | 20 分 + observer の全履歴 | 適用サンプル（8k が受け入れ中・sink 安定・gate が他の理由に隠れていない）のうち **factsDaa − registryTipDaa が最頻値のもの**で、`panelRoom == 0` ⇔ gate が拒否（rate の `the panel has no room …` **または** held 上限の `class … has N claims in flight against the registry's cap of 5`）が連続 2 回以上食い違わない、rate 拒否時の gate の inflight replay g（class の窓 W spans 分）と op 186 `panelInflightReplay` c（1 span 分）が同 DAA で `W·(c−1) < g ≤ W·c`、**fold が room/上限で attempt を拒否した行（`disqualified from virtual chain (PALW state): …` / merged blue の refused 行）が 0**、reason に `overloaded (… utilization …)` なし、`panelHorizonSpans = 1` |
| C13 | `licence` | ≥ 60 と窓の終わり | 各 2 分 | 直近 120 DAA に bind し 20 DAA 以上経った floor claim のうち、**稼働 seat の Valid receipt が 3 以上そろった claim（判定母集団、10 本以上）の licensed ≥ 95 %**。全 bound の比率も併記。**期待値 PASS**（licence-stall 修正 a4dfe903/d94d3a1b 入り。FAIL は c8652a97 に対する所見）。不合格なら `stuck-report.txt` に STUCK-WITH-QUORUM の claim・各 seat（カード/ノード、稼働中か、Valid を出したか）・licence 提出の有無 |
| C9-span | `reorg span` | +4 each side | 20–30 分 | 片側だけ `removed_chain_blocks > 0`、7 台が 1 sink、slashed/collateral が勝者と一致、disqualify/panic 0、compare rc ≠ 1、DNS overlay は `Bootstrap`（reorg gate 非強制）と記録、`fork.txt` に split DAA |
| C6 | `ibd` | 任意 | 5–15 分 | n7 が fleet の sink に到達、`chainstate genesis` 合格、disqualify/panic 0、compare rc ≠ 1 |
| C4-bind | `cheapcheck` | 任意 | 1 分 | 30 DAA（anchor 20 + 10）より古い floor claim がすべて bound、bind-timeout void 0 |
| C4-cheap | 同上 | x1 宣言後 | 1 分 | x1 宣言後に bind した floor claim が 10 本以上あり、そのどれにも x1 が seat として入っていない（10 本未満は NOT-REACHED、窓の終盤に再実行） |
| C16 | `maturity` | 窓内 | 1 分 | 窓内は NOT-REACHED（600 DAA を超えた coinbase がまだない） |
| C12-short | `GATE_SHORT=1 gate` | 窓の終わり | 2–5 分 | 直近 600 DAA に floor と 8k の algo 6、algo 9 が 0、どの node にも lane alarm なし（algo 10 は判定しない） |
| C15 | `second` | 窓の終わり | 1 分 | 第 2 の model class が Probation に入った後 10 DAA、in-flight の 8k が Held にならない（この roster では NOT-REACHED） |
| C9-final | `reorg final` | ~160–165 | 窓の最後 20–30 分（5.5 h 時点で DAA ≥ 150 のときだけ） | C9-span の条件に加え、split 以後に Final になった claim が勝者と同じ phase・quanta・execCredit で残る（split 以後に何も終わらなければ FAIL = 何もまたいでいない） |
| C10 | `silence` | **ソーク S3**（最初の 8k Final 後） | 数分 | 仕込みのみ（判定は C9-timeout）。held 8k は Final まで新しい claim を bind しないので窓内では走らせない |

### ~6 h の窓で届かないもの（理由つき、2.05 分/DAA ≈ 175 DAA）

- **C9-final**: 最初の floor Final = licence（≈ DAA 40–45）+ short challenge 120 ≈ **DAA 160–165 ≈ 5.5 h**。窓の最後に届くかもしれない（届かなければソーク S1）。
- **C11b（最初の 8k Final のメモリ）**: 8k licence（≈ DAA 70–85）+ 120 ≈ DAA 190–205 ≈ 6.5–7 h → 窓外。
- **C10（silence の仕込み）**: held な 8k 行は in-flight 5 本を Final まで保持するので、新しい bound 8k claim は最初の 8k Final（≈ DAA 190–205）が
  枠を空けた後（+ bind 20 DAA）にしか来ない → ソーク S3。
- **C11 route と (d) permit → algo-10 → payout**: Final の execution quanta は **window_challenge = 1,200 DAA 後**に成熟（`palw_exec_quantum_maturity_daa_v1`、short window の 120 ではない）→ 最初の permit ≈ DAA 1,360（span 丸め込み）≈ **46 h**。**窓内に algo 10 は 1 本も出ない**ので C12 の完全版と C9-exec も届かない。
- **C11-fee**: 資金は F0 が x3 に 20 MSK 用意する（x3 → drill main wallet に 5 回）。届かないのは algo 10 のほう（ソーク S6）。
- **C9-timeout ×2（課金と forfeit）**: silenced claim の bind（≈ DAA 215–230）+ 600 ≈ DAA 820（≈ 28 h）、2 回目はさらに + 600（≈ 49 h）。
- **C16 の本判定**: 最初の drill coinbase + 602 DAA ≈ 21 h。判定は node が `--coinbase-only` 送金を受理し block に載せたこと。「600 DAA 未満の coinbase を node が拒否する」側はこの CLI では組み立てられない（CLI が immature を選ばず、生 input の送金もない）ので **PASS 項目にしない**（CLI の算術として記録のみ）。
- **C15（第 2 class の audit）**: 2M 行は誰も着席できず、Candidate を登録する資金もないので、第 2 の model class が Probation に入ることがない。
- （届くようになったもの）**C3b と C4-cheap-bond**: F0 が drill main wallet から x1/x2 に資金を入れるので窓内で届く。C4-cheap は x1 の宣言後に floor の抽選が 10 回以上要る。

---

## 0. review 3（09-24、005e5c5f への retarget）で変わったこと

- **build**: 71c0399e（e93be0f2 + salt）→ **005e5c5f**（c8652a97 + salt + drill main wallet）。salt `…/DRILL-PRE-c8652a97`、premine `269a354e…`、genesis `32a665d6…`。
  禁止 genesis は 6 本とも全 hash（a27f8f44 を追加、fb1074b0・a8cabac4 も全 hash に、1 回目の drill の 1eaa6c0f も追加）、禁止 premine は 3 本（1 回目の drill の 65093cb6 を追加）。
  preflight の binary 検査、`chainstate genesis` の `--forbidden`/`--forbidden-prefixes`/`--forbidden-premines`、build-ship の genesis.rs 検査がすべてこの一覧を使う。
- **資金（F0）**: 所有者の手順だった F0 を**キットの手順**にした（`drill.sh fund`、b1 が C3a の直後に実行）。キットの fund wallet は keygen の乱数鍵ではなく
  **TESTNET_MAIN_SEED の鍵**（`drill.sh mainkey` が stdin で import、`keys/main.key`、FUND_KEY の既定）。keygen の wallet は 13 → 12 個（`fund` を削除）。
- **`assert_drill_key` は残した**: keygen の鍵、または import した main.key だけが署名できる。main.key は **premine txid が salt 済みだから**許され、
  毎回 `assert_drill_chain`（n0 が genesis 32a665d6… を DAA 0 で答える、drill premine 上の bond、公開物なし）を通った後だけ使われる。
  カード鍵・運用者の公開 main wallet に解決する鍵は従来どおり拒否。**すべての送金の直前**に `assert_drill_chain` を追加（F0、bond 登録、C3a の registrar、C4 の宣言、C16、C11-fee）。
- **C14**: 8k 行が held（上限 5、Final まで）になったので、gate の拒否に held 上限の文（`ClassInflightCapped`）を数える（op 186 の `panelRoom` は上限込み）。
  replay の比較は「gate は class の窓 W spans 分、op 186 は 1 span 分」の差を入れて `W·(c−1) < g ≤ W·c`（旧キットは等号比較で、W = 3 なら必ず食い違っていた）。
- **C5**: n2 の「戻った」判定に、held 上限での保持行を認める（上限 5 が埋まっていれば最初の 8k Final まで生産しないのが正しい動作）。
- **C10**: 窓内から外し、ソーク S3（最初の 8k Final の後）へ。
- **C13**: 判定は不変。licence-stall 修正入りなので**期待値 PASS**。
- **C11-fee**: 支払い元の優先順を x3（F0 の 20 MSK）→ FUND_KEY（main）→ 採掘 wallet に。x3 からの支払いは drill main wallet 宛て。
- **build-ship.sh**: 既定 `SHIP_REV=005e5c5f…`・`SRC_FROM=ship-pre2.bundle`、`CARGO_INCREMENTAL=0`、他の cargo/rustc が動いていれば開始しない、
  開始行に bundle の sha256 を記録。source marker 33 → 48 個（`palw_panel_room_by_rate_v1` は c8652a97 で方法名と `palw_panel_capacity_by_rate_v1` に置き換わったので差し替え）、
  binary marker 15 → 18 個（`held: its verification window (` は c8652a97 で文言が変わったので `held: seats and window are back; …` と ` spans) does not fit the receipt deadline` に）。
  `PALW_RCORE_C7_WINDOW_SPANS_V1` は 005e5c5f に**存在しない**ので marker にしていない。

## 0'. review 2（09-24）で変わったこと

- **トポロジ**: `--connect` を 1 つでも渡すと inbound 0 → client-only で listener を開かない（daemon.rs:1046、service.rs:68）。旧キットは全 node が `--connect` を持ち、**1 本も接続できなかった**。hub（n0, n3）は `--outpeers=0` で 127.0.0.1 に listen し、n0 だけ `--addpeer=n3`。leaf（n1 n4 n5 n7 n8 → n0、n2 n6 → n3）は `--connect` のまま（client-only なので誰からも dial されない）。partition は n0 を `--addpeer` なしで再起動。`start_node` は hub の LISTEN を assert する。
- **HOME**: kaspad は起動のたびに `~/.misaka/testnet-12/endpoints.json` を書く。kaspad と CLI を `HOME=/root/drill-t12/home` で動かし、root の registry（live t12 node のもの）は preflight で sha/mtime を記録、invariants と down で比較。
- **環境**: KASPAD_*/MISAKA_* をすべて unset し、kaspad は `env -i` で起動。
- **pid**: pid ファイルは `/proc/<pid>/cmdline` に `--appdir=$RUN/n<i>`、`/proc/<pid>/exe` が drill の kaspad のときだけ信じる。observer も cmdline を照合。
- **artifact**: `/root/perm-drill` は読まない（`stage-art` は廃止、`verify-art` に置き換え、perm-drill に解決するパスは拒否）。A2 で Mac から scp。
- **registry state**: RPC は `{:?}` なので `Probation { probes_passed: 0 }`。C7・C14・C15・observer は variant 名で比較。
- **C13**: 判定母集団を「稼働 seat の Valid が 3 以上そろった bound claim」に。x2 は既定で floor を宣言しない。
- **C16/C11-fee**: `mature` は CLI の算術。判定は node の受理と block への取り込み。`--recent 3000`。
- **C8**: 最新の readiness 事象が proof であること（一時的な `no proof` で落とさない）。
- **C3a**: key モードの bond status は `bonds_registered_to_this_key`/`bonds_unanswered`。
- **C14**: offset 一定のサンプルだけで判定、fold の room 拒否行 0 を PASS 条件に、gate の inflight replay と op 186 の一致。
- **C1**: fingerprint を全 node の RPC から。**C9**: split DAA を記録し、split 以後に終わった claim だけを数える。

## 1. 前提にした事実（005e5c5f のコードで確認済み）

- **identity**: premine txid は `BLAKE2b-512(key="misaka-premine-txid/v1", sentinel ‖ "testnet-12" ‖ salt)`（premine.rs `network_separated_txid`）。drill の値 `269a354e…`、公開の値 `5e0d5f1b…`、1 回目の drill の値 `65093cb6…` をすべて Python で再計算して一致を確認。t12 の community txid も同じ salt で分離（`testnet12_community_txid`）。カード N の bond は `premine_outpoint_for(net, N)` = `269a354e…:N`、fee float は `MAIN_PREMINE_INDEX + 1 + N` = `:41+N`（各 100 MSK、card の payout 宛て。**payout は bond 鍵自身の address**）。genesis bond collateral は各 939,063.21 MSK で main wallet の spk（lock 済み、CLI は選ばない）。genesis の各 hash は各 commit の genesis.rs から全 hash で抽出（a27f8f44 = c8652a97、d73dbf44 = e93be0f2、fb1074b0 = 2bd134ec、f6cc9576 = b791b460、a8cabac4 = 2bd134ec^、1eaa6c0f = 71c0399e、32a665d6 = 005e5c5f）。**drill genesis の hash と utxo commitment は drill commit の値をそのまま使った（ここでは再計算していない）**。食い違えば kaspad が起動時の genesis mismatch guard（utxo_set_override.rs）で止まり、C1 で判明する。
- **drill main wallet**: 005e5c5f の `main_address_for` は testnet-12 に対して `return TESTNET_MAIN_ADDRESS;`（drill 専用の override、source marker）。鍵は `tests::TESTNET_MAIN_SEED = b"misaka-testnet-premine-9b-claude-managed"` の BLAKE2b-256 を ML-DSA-87 seed にしたもの（`testnet_main_key_is_reproducible`、CLI の `ValidatorKey::from_seed` と `funding_address` と同じ導出）。CLI の import は `misaka key import --out <path> --hex-stdin`（main.rs `key_import`、keys.rs `import`: stdin か 0600 のファイル、argv は不可、O_EXCL で上書きしない）。Mac で同じ経路を空打ちして address = TESTNET_MAIN_ADDRESS を確認。非 coinbase の premine output は即 mature（`is_spendable_settled`）。`wallet send` は最大の selectable output から選ぶので、F0 は 1 本ずつ確定を待つ。heartbeat block は通常の template から作られるので mempool の tx を載せる（F0 は producer 起動前でも確定する）。
- **P2P**: `--connect` ⇒ `outbound_target = 0`・`inbound_limit = 0`（daemon.rs:1045-1046）⇒ `Adaptor::client_only`（service.rs:68、adaptor.rs:49 は listener を開かない）。`--addpeer` は恒久の接続要求で `--outpeers` に関係なく dial される（connectionmanager `handle_connection_requests`、backoff 30 s·2^n、最大 8 分）。loopback は address manager に入らない（addressmanager lib.rs:271）。`--connect` と `--addpeer` の併用は起動拒否（daemon.rs:127）。自動 ban は RPC の `ban` からしか起きない（P2P flow は ban しない）。
- **fingerprint**: `consensus_params_id` は `genesis` を hash に含むので、drill の fingerprint は公開 genesis を持つどの build とも一致し得ない。RPC は `getPalwNodeStatus.consensusParamsId`、ログは `Consensus params fingerprint: <hex> (network testnet-12)`（daemon.rs:736）。
- **genesis の RPC 確認**: `getBlockDagInfo` は genesis を返さない。`getBlock(drill genesis)` が daaScore 0 で答え、禁止 genesis 6 本の `getBlock` がエラーになり、`getPalwClaims(<premine>:1).bondKnown` が drill の txid で true・禁止 premine 3 本で false、を `chainstate.py genesis` が 1 回で判定する。`assert_drill_chain` はさらに「DAA 0 で答える」（pruned-past を認めない）を要求する。
- **CLI**: `misaka` は bond 登録と `bond capability` を `params.genesis.hash` に対して署名する（misaka-cli/src/bond.rs:62）。キットは `/root/drill-t12/bin/misaka` だけを使う（`BIN_DIR` は上書き不可）。wallet 系は network id だけを照合する（wallet.rs `connect`）。misaka-cli は e93be0f2 → c8652a97 で変更なし。
- **endpoint registry**: `registry_path = dirs::home_dir()/.misaka/<network-id>/endpoints.json`（misaka-endpoints/src/lib.rs:60）を kaspad が起動時に書く。CLI は `--rpc` がないとそれを読む。
- **replay（item 6）**: ML-DSA の sighash は outpoint を commit するが network と genesis は commit しない。salt で premine 由来（と community）の outpoint はすべて drill 固有。drill main wallet（公開テスト鍵）が使うのは `269a354e…:40` とその drill chain 上の子孫だけで、drill chain 上で TESTNET_MAIN_ADDRESS に払う coinbase はない（producer は pay-*、clock は hb-*、round は card の payout）。入力のない coinbase だけが残る穴で、block を作る経路は producer（`--palw-producer-pay-address`）、heartbeat miner（`--palw-heartbeat-miner-address`）、round producer（bond の payout、ただし round block は selected parent にならず coinbase は UTXO に入らない）。キットは前 2 つを drill 専用 address にし、送金はすべて drill の鍵（keygen の 12 個と main.key）から `assert_drill_chain` の後に行う。
- **窓**: `PALW_RC_WINDOWS_V1` = bind 600 / receipt 600 / challenge 1,200 / court 3,000 / **anchor_delay 20**。t12 は short challenge window（120 DAA）を DAA 0 から適用。execution quanta の成熟は `window_challenge()` = 1,200。
- **bond の floor（a3c5db22）**: 13,000 MSK が producer floor かつ登録 floor（`palw_bond_registration_floor_v1`）。kaspad の `size_bond_collateral` は**資金を探す前に**拒否する。seat の floor は 130,000 MSK、readiness は 39,000 MSK の空き。C4 は「13,000 MSK の bond は floor を宣言しても抽選されない」を、x1 宣言後の floor 抽選 10 回以上で見る（x1 が有資格なら 1 回あたり約 5/8 の確率で入る）。
- **DNS（item 8）**: `PALW_T12_DNS_PARAMS` = 6 validator × 20,000,000 MSK。drill chain にそんな validator はいないので overlay は `Bootstrap`（reorg gate を強制しない）。coinbase の成熟は `coinbase_settlement_long_maturity_daa = 600` の DAA だけ（Decision A）。
- **heartbeat（H1–H5、item 9）**: miner は peer がいないと保持する（`has_peers()`）。各 beat を `— granted: … advanced the clock to DAA N` / `holds the open slot, not granted yet` / `NOT granted: stamped before its slot` で出す。C1 が 3 種を数え、granted 行の時刻から pace を出す。
- **lane watch**: `no PALW work block in the **newest** N selected-chain blocks`。30 block 未満では鳴らない。selected chain しか数えないので、ゲートは DAG 全体を数える `chainstate.py lanes/gate` で判定する。
- **C7**: Prefetching の行のログは `holding: the model registry holds class <id>: Prefetching since span N`（palw_producer.rs:853）。facts の `notReadyReason` は `the model registry admits no new claim of this class now [<gate の文>]`（service.rs:2864、他の理由が先に立つと gate の文は出ない）。
- **registry の state**: `format!("{:?}", r.state)`（service.rs:1891）。`Probation { probes_passed }`・`ActiveLimited { stable_epochs }` は field を持ち、`Held` などは単位 variant。Held の reason は c8652a97 で `held: {window}; and {seats}` などの組み立てに変わった（`held: its verification window (` という文字列はもうない）。
- **panel room（item 10b）— c8652a97**: `palw_audit_2026_09_23` 以後、room は class ごとの窓の rate（`panel_room_by_rate_v1`、`PalwPanelRateV1`、`palw_panel_capacity_by_rate_v1`）。**held class**（`class_is_held_v1`: t12 の 8k 行 `ebf44d0a…`＝窓 3 spans・上限 5、2M 行 `74c67e63…`＝窓 2,799・上限 1）は claim を Final まで予算に載せ（`palw_panel_owed_v1`）、fold の gate は**先に** held 上限を見る（`ClassInflightCapped`: `class {class} has {inflight} claims in flight against the registry's cap of {cap}`）、次に rate（`PanelRoomExhausted`: `the panel has no room for a claim of class {class}: {inflight_replay} of replay in flight against a budget of {budget} over {horizon_spans} spans`、horizon = その class の窓 W、inflight = ceil(D·W/S)）。op 186 は `panelRoom` = min(rate room, 上限 − owed)（`palw_panel_room_read_v1`）、`panelInflightReplay` = ceil(D/S)、`panelHorizonSpans` = 1。producer の事前確認は op 186 の room を読み、0 なら `holding: class … has no panel room left in the network's verification budget`（palw_producer.rs:864）。chain block 自身の attempt が拒否されると `disqualified from virtual chain (PALW state): …`（processor.rs:2172）、merge された blue の場合は `PALW: merged blue … carried work this chain point refused …`（processor.rs:2154）。
- **licence stall（item 10a）— c8652a97**: a4dfe903（遅れて来た満席の seat でも licence する、coverage licence が形成される、re-bound した panel は再判定: `palw_select_optimistic_licence_v2`・`palw_select_coverage_licence_v2`、answered/replayed の key が `(claim, bound_daa, panel_anchor)` = `seat_duty_panel_key_v1`）、d94d3a1b（lock を出せない rider は飛ばす、fold の filter は全 network で走る、V3 receipt pool は `RECEIPTS_V3_MAX_CLAIMS = 256` で上限）。ログの文言は変わっていない。
- **receipt と readiness のログ**: `filed a "Valid" [V3 ]receipt for claim <id>`（palw_panel.rs:5571/5601、`{:?}` なので引用符つき）、`submitted a readiness proof for class <id>`、`submitted {ReceiptLicensed|ReceiptLicensedV2|OptimisticLicensed} for claim …`（palw_panel.rs `object_name`、C13 は `Licensed` を含む submitted 行を数える）、readiness の note は変化したときだけ出る。
- **状態の一致**: チェーン block は親の `palw_state_root` を commit しており、不一致は `disqualified from virtual chain (PALW state root)`。「全 node が同じ sink、この行が 0」が最強の証拠。
- **ReceiptTimeout の課金**: absent seat に `claim.reserved` を課金、2 回目の ReceiptTimeout で producer の weight + escrow を forfeit。

## 2. host・ポート・調整（5.104.81.23）

**ここ以外では走らせない。** misaka-ibm と 169.58.232.113 には公開 node がある。

| 何 | ポート / パス | 所有 | ドリル中の扱い |
|---|---|---|---|
| `misaka-t12f-b2..b5`（**live** floor seat、公開チェーン） | 26511–26543 | 別 session | **触らない**。RSS 計 3.4 GiB を確保し続ける |
| `misaka-t12p-0..7` + scan + tunnel + daa-obs（private t12） | 27501–27596 | 別 session | **合図＝これが止まったこと**。preflight は active なら拒否 |
| t11 kaspad pid 2990090 | 36311–36313 | 別件 | 触らない（pid 照合により signal は drill の kaspad にしか届かない） |
| python ws poller | 26313 | 別件 | 触らない |
| `/root/t12-private/keys` | — | 所有者 | カード鍵の**読み取りのみ**、この host 上、この drill chain だけ、RPC は 127.0.0.1 のみ。コピーしない・中身を表示しない（address の導出だけ）。**送金には使わない** |
| `/root/drill-t12/keys/main.key` | — | このキット | drill main wallet（公開テスト鍵 TESTNET_MAIN_SEED、0600）。`assert_drill_chain` を通った送金にだけ使う |
| `/root/perm-drill` | — | 別 session | **一切読まない**（review 2）。artifact は A2 で Mac から |
| `/root/.misaka/testnet-12/endpoints.json` | — | live t12 node | 書かない（drill は `HOME=/root/drill-t12/home`）。preflight で記録、invariants と down で比較 |
| **ドリル** | **28601–28693**（node i: P2P `286i1` / borsh `286i2` / json `286i3`） | このキット | 全部 127.0.0.1、`--nodnsseed`、hub は `--outpeers=0`、leaf は `--connect` |

## 3. roster とトポロジ

| node | bond | pay address | 役割 | P2P |
|---|---|---|---|---|
| n0 | カード 0 | `pay-0`、heartbeat `hb-0` | clock A + 観測の基準 + 8k seat + round lane + **送金の経路（`assert_drill_chain` の対象）** | **hub A**: listen、`--outpeers=0`、`--addpeer=n3`（partition 中はなし） |
| n1 | カード 1 | `pay-1` | floor producer（FP 認証なら canonical FP）+ 8k seat + round lane | leaf → n0 |
| n2 | カード 2 | `pay-2` | **8k producer** + 8k seat + round lane | leaf → n3 |
| n3 | カード 3 | `pay-3`、heartbeat `hb-3` | clock B + floor producer + 8k seat + round lane | **hub B**: listen、`--outpeers=0`、dial しない |
| n4, n5 | カード 4, 5 | `pay-4`, `pay-5` | 8k seat + round lane | leaf → n0 |
| n6 | カード 6 | `pay-6` | 8k seat + round lane。**C7 まで起動しない** | leaf → n3 |
| n7 | — | — | IBD follower（一時、`--ram-scale 0.3`） | leaf → n0 |
| n8 | x1 / x2 | 自分の address | bond registrar（一時、`--ram-scale 0.1`）。資金は F0 が入れたもの | leaf → n0 |

カード 7 には node を立てない（floor panel の「黙る seat」、C7 までは カード 6 も黙る）。quorum 3/5。C13 は Valid が 3 以上そろった
claim だけで判定するので、黙る seat による構造的な不成立と欠陥を分けられる。

## 4. ソーク（窓の後も続ける場合の順、2.05 分/DAA）

| # | コマンド | 目安 DAA | 壁時計（genesis から） |
|---|---|---|---|
| S1 | `FINAL_BOND=1 LEAD=3 ./drill.sh reorg final`（窓内で済んでいなければ） | ~160–165 | 約 5.5 h |
| S2 | `./drill.sh c2watch` | ~190–205 | 約 6.5–7 h |
| S3 | `./drill.sh silence`（最初の 8k Final が枠を空け、新しい 8k claim が bind した後。rc=2 なら次の 8k claim で再実行） | ~215–230 | 約 7.5–8 h |
| S4 | `./drill.sh maturity`（本判定） | ~605 | 約 21 h |
| S5 | `LEAD=3 ./drill.sh reorg timeout`（1 回目: 課金） | silence の bind + 600 ≈ 820 | 約 28 h |
| S6 | `./drill.sh route`（C11 → C11-fee。支払いは x3 の F0 資金から） | ≈ 1,360 | 約 46 h |
| S7 | `LEAD=3 ./drill.sh reorg timeout`（2 回目: forfeit）→ `./drill.sh unsilence` | ≈ 1,420 | 約 48.5 h |
| S8 | `./drill.sh reorg exec` → `./drill.sh ibd` | ≈ 1,430–1,450 | 約 49–50 h |
| S9 | `./drill.sh licence` → `./drill.sh cheapcheck` → `./drill.sh gate` → `./drill.sh down` | ≈ 1,460 | 約 50 h |

F0 は冪等: `./drill.sh fund` を再実行しても、bond 済み・資金済みの wallet には送らない。`./drill.sh b1` の再実行は x1 登録済みなら何もしない。

## 5. チェック項目（PASS / FAIL / NOT-REACHED / 証拠）

共通（C0 の不変条件。各コマンドの最後に `invariants`）: 起動した node が落ちていない（pid は drill の kaspad と照合）、`panicked at` 0、
`disqualified from virtual chain (PALW state root)` 0、**稼働中のすべての node で `chainstate genesis` 合格**、root の endpoints.json に
drill の port が書かれていない。証拠は**すべて行動の後のログ offset から**取る（`mark`/`since`）。`NOT-REACHED` は「この run では chain
（または資金）が届かなかった」で、FAIL ではない。**送金はすべて `assert_drill_chain` の後**（F0 の項）。

| ID | 何を証明するか | 証拠（`ev/…`） |
|---|---|---|
| **C0** preflight | drill build で走ること、identity が drill であること、環境が隔離されていること | `C0-preflight/{sha256,identity-bytes,env-leftovers,public-endpoints-before,drill-keys,art8k-sha256,art8k-manifest-check,host}.txt` |
| **C1** boot | drill genesis・drill premine・非公開 fingerprint（RPC）・hub の LISTEN・カード鍵の一致・drill main wallet の `:40`・clock・pace | `C1-boot/{identity-n*.json,hub-listen,fingerprints,cards,heartbeats,pace}.txt`, `main-wallet.json`, `dns.json`, `registry.json` |
| **F0** fund | drill chain が証明された後だけ、drill main wallet（公開テスト鍵）が drill premine の `:40` から x1..x3 に資金を入れること | `F0-fund/{identity-n0-*.json,main-utxos-*.json,main-wallet.txt,sends.tsv,sends.log,x1-after.json,x2-after.json,x3-after.json,main-utxos-after.json}` |
| **C2** 起動ゲート（負例） | heartbeat だけのチェーンが警報を出し、ゲートが拒否すること | `C2-alarm/{status-alarmed,gate-heartbeat-only,status-cleared}.json` |
| **C3a** 登録 floor | 13,000 MSK 未満の bond は資金が動く前に拒否され、chain に出ないこと | `C3-b1/x1-{cheap,floor-minus-1,at-floor-unfunded}.log`, `x1-bond-status-after-refusals.json` |
| **C3b** B1 | 新規鍵が drill build の builder で floor ちょうどの bond を作れること（資金は F0） | `C3b-fresh-bond/{x1-registrar,x1-chain}.log`, `x1-bond-status.json` |
| **C4-bind** | floor claim がすべて bind し、bind-timeout void がないこと | `C4-bind/{verdict.txt,floor-claims-*.json}` |
| **C4-cheap-bond** | 13,000 MSK（< seat floor 130,000）の bond は floor を宣言しても抽選されないこと（宣言後 10 抽選以上） | `C4-cheap-bond/{verdict.txt,bonds.txt,x1-as-seat.json,x2-as-seat.json}` |
| **C5** 請求途中の再起動 | 8k producer と抽選 seat を bind 後に再起動しても claim が licence し、n2 が戻る（生産、または held 上限で保持） | `C5-restart/{pick.txt,n2-back.log,seat-after-restart.log,claims-bond2.json}` |
| **C6** IBD | genesis からの同期、fleet との一致、drill identity | `C6-ibd-*/{ibd.txt,identity-n7.json,compare.json,bad-lines.txt}` |
| **C7** #7 | 受け入れない class の producer が保持し、facts と CLI が `E-MODEL-NOT-ADMITTING` を言うこと | `C7-hold/{n2-holding.log,facts.json,mining-status.json,8k-before.json,8k-after.json}` |
| **C8** 3.5 GiB | 8k seat が share 3.5 GiB で ready になり、最新の readiness 事象が proof であること | `C8-readiness/{n*-proofs.txt,n*-status.json,registry-readiness.json}` |
| **C9** reorg ×4 | span・Final・課金・forfeit・execution をまたぐ reorg で二重課金・二重 forfeit・二重 mint がないこと | `C9-reorg-<mode>-*/{fork.txt,dns-{pre,post}.json,slashed-*.json,claims*-{A,B,post}.json,compare.json}` |
| **C10** 課金源 | C9-timeout が課金と forfeit を実際にまたぐこと | `C10-silence/*` と C9 の timeout ディレクトリ |
| **C11** 実行ルート | Final → scheduled（Final + 1,200）→ round block → permit | `C11-exec-route/{1-first-finals,2-scheduled,3-round-lane}.json` |
| **C11-fee** | drill wallet（x3、F0 の資金）からの送金を node が受理し round block に載り、merge した chain block の coinbase が round payout を払うこと | `C11-fee/{4-source-utxos.json,4-maturity.txt,5-sends.txt,6-carried.log,7-fee.json}` |
| **C11b** C-2 の回帰 | 最初の 8k Final で anon 増加 < 512 MiB、1 claim の execTickets ≤ 120 | `C11b-model-final-memory/*` |
| **C12** 起動ゲート | class ごとのレーン（short 版は algo 10 を判定しない） | `C12-gate[-short]/{gate,lanes}.json` |
| **C13** licence の生存性（item 10a） | Valid が 3 以上そろった bound floor claim の ≥ 95 % が licence する（**期待値 PASS**） | `C13-licence-*/{licence.json,verdict.json,stuck-report.txt}` |
| **C14** panel room（item 10b） | op 186 の `panelRoom` が gate（rate の room、または held 上限）と一致、`panelInflightReplay` × 窓が gate の replay と一致、fold の room/上限拒否 0、utilization が class を Held にしない | `C14-panel-room/{samples.jsonl,observer-room.jsonl,room-refused-attempts.txt,verdict.txt}` |
| **C15** 第 2 class（item 10c） | 第 2 class の audit 通過が in-flight の第 1 class を Held にしないこと | `C15-second-class/verdict.txt`（`run/classes.tsv` から） |
| **C16** coinbase 成熟（item 8） | 600 DAA を過ぎた coinbase の `--coinbase-only` 送金を node が受理し block に載せること | `C16-coinbase-maturity-*/{cli-arithmetic.txt,send.txt,x3-after.json,*.json}` |

## 6. メモリ・CPU・ディスク（5.104、perm-drill の実測値ベース）

| 対象 | anon | RSS | 備考 |
|---|---|---|---|
| 8k seat（share 3.5 GiB）×7 | 0.56–0.80 GiB | 2.3–2.5 GiB | artifact 1.68 GiB は file-backed で page cache に 1 部だけ（`/root/drill-t12/art` のコピー） |
| 8k producer の上乗せ（n2） | +1.2 GiB（ピーク 1.95） | 3.7 GiB | attempt の prefill は 1 core で約 290 s |
| 8k replay の一時領域 | +1.67 GiB/seat | — | 1 claim に 5 seat → 最悪 +8.4 GiB（ledger が node ごとに 3.5 GiB で抑える） |
| IBD n7 / registrar n8 | ~0.3 / ~0.2 GiB | — | 一時的 |
| live floor seat ×4（他人） | — | 3.4 GiB | 必ず残す |

定常 ≈ 12.7 GiB、ピーク ≈ 21 GiB（23 GiB の host）。**t12p の停止が必須**（preflight は MemAvailable 17 GiB 未満なら拒否）。
`observe` の非常停止: MemAvailable < 2.5 GiB が 3 分続いたら**ドリルの node だけ**を止める（pid 照合つき）。ディスク: target ~20 GB + datadir 2–5 GB/node + art 1.8 GB。
build（A1）は `-j4`・nice 10・`CARGO_INCREMENTAL=0` で MemAvailable ≥ 8 GiB と空き ≥ 40 GB を確認してから始め、他の cargo/rustc があれば始めない。

## 7. 未決事項（ユーザー判断）と制約

- **Q1 資金**: 解決済み（review 3）。F0 はキットの手順で、drill main wallet（公開テスト鍵 TESTNET_MAIN_SEED、drill chain の salt 済み `269a354e…:40`）から
  x1 13,001 / x2 135,001 / x3 20 MSK。`assert_drill_chain` の証明なしには 1 sompi も動かさない。カード鍵による送金はキットが拒否する。
- **Q2 時間**: 窓内で届くのは C0–C8・F0・C3b・C4・C13・C14・C9-span・C6・C12-short、条件つきで C9-final。permit → algo-10 → payout は最短でも Final + 1,200 DAA（≈ 46 h）。
- **Q3 algo 9**: t12 では武装されない。ゲートは algo 9 を「出たら FAIL」として扱う。
- **Q4 algo 7**: FP 認証済みの class で canonical FP を回した場合だけ出る。`facts.fpCertified` で自動判定。
- **Q5 課金源（C10）**: 3 seat から artifact を外すと 8k の ready seat が 4 になり Held になる。rebind は NoCapablePanel になりうるので、C9-timeout の「二重課金なし」は偶発的な timeout 待ちか unit 証拠に頼る可能性がある。held 8k は上限 5 を Final まで保持するので、silence は最初の 8k Final の後（S3）。
- **Q6 公開 fingerprint**: c8652a97 の公開 release の fingerprint は未記録。判明したら `PUBLIC_FP=<hex8> ./drill.sh up` で denylist に加える（構造上一致はしない）。
- **Q7 artifact の経路**: A2 は Mac の `deploy-t12/.cache/art` からの scp（sha256 を固定）。別経路にする場合は `ART8K_SHA256`/`ART8K_MANIFEST_SHA256` を渡す。
- **Q8 drill genesis の再計算**: `32a665d6…`/`d50a03ca…` は drill commit の値をそのまま採用（Mac はディスク残 16 GiB で test build をしていない）。誤りなら kaspad が起動時に止まる（C1 の最初の数秒で判明、rebuild が要る）。

## 8. 停止

`./drill.sh down` は pid 照合を通ったドリルの node と observer だけを止める。systemd unit には触れない。`/root/drill-t12` はそのまま残す。
down の最後に root の endpoints.json が preflight 時と同じか（または drill の port を含まないか）を記録する。
drill 専用鍵 `/root/drill-t12/keys/*.key` は drill chain 以外で使わない。`keys/main.key` は公開テスト鍵で価値はないが、
キットは `assert_drill_chain` を通らない node にこれで署名した tx を送らない。
