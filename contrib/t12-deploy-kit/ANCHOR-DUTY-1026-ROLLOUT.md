# testnet-12 node-only release: anchor duty の短い待ち + 5.104 の b5 退役 — rollout checklist

2026-10-26 作成(branch `release/t12-anchor-duty-247ade1f9`)。

> **実施状況(2026-10-26 09:00〜09:24 CET)**: §4 の手順 0 と 1 まで実施した。release branch を push、`distribute-from-mac.sh kit` と
> `binaries`(3 台とも sha256 一致)、各 host で `stage` と `DRY_RUN=1 upgrade`。DRY_RUN は 3 台とも最後まで通り、`nothing … was changed`。
> 稼働中の node・pinner は 1 台も再起動していない(全部 247ade1f90b6 のまま)。各 host の旧 `fleet.env` は
> `/root/t12-rel/kit/fleet.env.bak-before-24d7560777f0`、5.104 の旧 `install-5104.sh` は同じ名前の `.bak-…` に残した。
> **手順 2(ibm)実施済み(2026-10-26 09:43〜09:52 UTC)**: pinner → b0 → b1 → b3 の順に `upgrade`。全 node `UPGRADED`(停止は
> `result=success`、gate は fp・genesis・facts・DB 保持・synced・DAA 前進のすべて通過)、ibm の 3 node と pinner は `24d7560777f0/bin/kaspad`
> で動作。`CHECK_REGISTRY=1 ./check-fleet.sh` を 09:54 と 09:57 UTC に 2 回: 7 node とも `RESULT: OK`、DAA 15,986 → 15,987、8k
> `readySeatsNow 9 / 7`。Mac からの seeder・anchor の確認は作業環境の通信制限で FAIL/CLOSED になったため 5.104 から確認した(anchor 3 つ
> open、seeder 4 台とも両 anchor を返す)。各 seat の `oldest duty has waited ~590 DAA` は upgrade 前からの panel の積み残し。
> **手順 3(.113)以降は未実施。**
>
> **kit の修正(この過程で見つけたもの)**: host の kit の `lib.sh`(main と同じ)は Phase H の IR artifact(`ART_PH`、
> `PH_MANIFEST_MODE=drop`)と `HOST_EXTRA_ARGS` を launch script に入れない。live の 247ade1f90b6 の launch script は、Mac に残っていない
> int-8 Phase H 版の kit で生成されたもので、全 8k seat に `--palw-class-artifact=…smollm2…palwtir` があり、`--palw-verify-class-manifest`
> は無く、5.104 では末尾に `--palw-seat-replay-slots=1` が付く。最初の DRY_RUN はこの差で pre-flight 中止になった(何も止まっていない)。
> この branch の `lib.sh` にその 2 つを足し、stage し直した結果、live と新しい launch script の差は header・`BIN`・`EXPECT_SHA` だけ
> (5.104 の b2・b4 は意図した `--addpeer=127.0.0.1:26341` の削除も)になった。`main` の kit には同じ欠落が残っている。

元の記述(作成時点では、この文書のどの手順もまだ実行していなかった): 実行したのは Mac 上の
build・テスト・再 pin の dry run と、live への読み取りだけ。背景と測定は `docs/design/palw/t12-panel-room-bind-loop-1026.md`。

## 1. このリリースに入っているもの

公開 fleet が動かしている `247ade1f90b6`(int-13 list を DAA 15,500 で武装した release、live は 15,500 を通過済み)に、次の 2 commit
だけを載せたもの。`main` にある 247ade1f9 以降のコード差分(`palw_panel.rs`・`daemon.rs`・`palw-attempt-exec` など)は **入れていない**。

| 中身 | consensus か |
|---|---|
| anchor duty の短い待ち: どの REAL class も class gate で新しい attempt を受け付けないとき、floor producer の binder は 4 slot + stagger で出る(従来は常に 30 + stagger)。producer facts の `real_attempts_refused`、`palw_accept.rs`、`palw_producer.rs` | **いいえ**(node policy。rule・fingerprint は変わらない) |
| kit: 5.104 から b5 を外す(`install-5104.sh` の NODES、`move-nodes.sh` の RETIRES)、`check-fleet.sh` の表示 | いいえ(kit) |
| テストの修正(`tests.rs` の walk テストのコンパイル) | いいえ(テストのみ) |

効くのは floor producer を動かす **ibm b1 と .113 b6**。他の node は同じ binary に揃えるために更新する。

## 2. identity — 変わらない

`scripts/release/t12-repin.sh --drift-only`(この tree の dry run): **drift なし**、361 ok、すべての gate が通り、未登録の literal もない。

| 項目 | 値(live の `247ade1f90b6` と同じ) |
|---|---|
| fingerprint(`EXPECT_FP`、起動ログ `Consensus params fingerprint`) | `ef379378f09bf33f29e97854bbee157a8f86fa1dd8248e1a847f08c5c6e7f15e` |
| schedule id(handshake で比べられる fork id) | `9ef2b1a795c74d6faced545752f2d17c7e731e8e94155d078bc00cbee6c878f5` |
| genesis(`EXPECT_GENESIS`) | `a27f8f44…a8ca1f23` |
| premine txid(`PREMINE_TXID`) | `5e0d5f1b…e55e2669` |
| rule manifest digest | `9def81a1…` |

`fleet.env` の `UPGRADE_FROM_FP` は空(稼働中の node は `ef379378…` を答えている。2026-10-25 09:09 の node0 の起動ログで確認)。
Mac では x86_64 の binary を probe できず(docker も無い)、`.cache/24d7560777f0/IDENTITY` は live と同じく repin の値から書いた。
**IDENTITY は binary を実行して得た値ではない。** 実行した値の確認は、各 host の `install-<host>.sh stage` と、`upgrade` 後の fp 確認で行う。

## 3. binary

`build-release-local.sh 24d7560777f0…`(Mac、cargo-zigbuild、x86_64-unknown-linux-gnu、glibc floor 2.39、369 s)。出力は
`contrib/t12-deploy-kit/.cache/24d7560777f0/`(git 管理外)。

| binary | sha256 | 247ade1f90b6(live) |
|---|---|---|
| kaspad | `0a7d4bbc08833acbb33d63c1f9ce71bec19859d6b5913ea30e31089efbe5fa3e` | `c57c8c13…` |
| misaka | `758f6125f0a8e61c25c4561e7e449ea2c15f4ff0f2368f8a5a2c578fcabfe5e4` | `eeee25bb…` |
| palw-class | `fb8629fb55cde6bb697da748b3e3dca1fb518b079125068ae47930cf60fd60b1` | `9a435da4…` |
| misaka-dnsseeder | `a96b686231ccf33493236d286fbd745f6300a69b77990473d1b2708e41c18216` | 使わない(`SEEDER_SHA256=KEEP`) |

`fleet.env`(Mac の kit、git 管理外)は live の `fleet.env` から `REV`・3 つの sha256・`UPGRADE_FROM_FP`(空)だけを変えたもの。
commit `24d7560777f0` はまだどの remote にも push していない(build script の注意)。fleet に入れる前に push する。

## 4. 手順(1 host ずつ。PLAN.md §15 の `upgrade` / `upgrade-rollback`)

前提: §15 のとおり、ibm b0 と .113 b6 が heartbeat miner(両方同時に止めない)、b0 が唯一の 8k producer、8k class は ready 9 / 必要 7。

| 順 | どこで | 何を | 次に進む条件 |
|---|---|---|---|
| 0 | Mac | `fleet.env` の `REV`・`KASPAD_SHA256`・`MISAKA_SHA256`・`PALW_CLASS_SHA256` を §3 の値に、`UPGRADE_FROM_FP` を空にする(他の行は live の `fleet.env` のまま)→ `./distribute-from-mac.sh kit` →【確認】`./distribute-from-mac.sh binaries` → `./check-fleet.sh` | 全 node が fp OK・synced・peer あり |
| 1 | 各 host | `./install-<host>.sh stage` → `DRY_RUN=1 ./install-<host>.sh upgrade`(稼働中の service に触れない)。5.104 だけは `DRY_RUN=1 UPGRADE_ARGS_CHANGE_OK=1 ./install-5104.sh upgrade`(b2・b4 の peer から `127.0.0.1:26341` が消える diff が出る。それ以外の diff が出たら止める) | ibm・.113 は ARGS の diff なしで最後まで通る |
| 2 | ibm【確認】 | `./install-ibm.sh upgrade`(先に host の pinner `misaka-palw-pinner` を入れ替えて再起動し、その後 b0 → b1 → b3、20 s 間隔。.113・5.104 も同じく pinner が先) | `./install-ibm.sh check` と Mac の `CHECK_REGISTRY=1 ./check-fleet.sh` を 2〜3 分あけて 2 回: daa が進む・8k `readySeatsNow ≥ 7` |
| 3 | .113【確認】 | `./install-113.sh upgrade`(b6 → b7) | 同上。misakascan の表示が進む |
| 4 | 5.104【確認】 | `UPGRADE_ARGS_CHANGE_OK=1 ./install-5104.sh upgrade`(b2 → b4。b5 は NODES に無いので触らない — 旧 binary のまま動き続けるが identity は同じ) | `CHECK_REGISTRY=1 ./check-fleet.sh` |
| 5 | 5.104【確認】 | b5 の退役: `distribute-from-mac.sh kit` は `move-host.sh`・`move-nodes.sh` を送らないので、先に Mac から 5.104 の `/root/t12-rel/kit/` へ送る → `./move-host.sh preflight retire 5` → `MOVE_TO=retired CONFIRM_MOVE_SEAT=yes ./move-host.sh retire 5` | 8k の ready が 8 に落ちて止まる(必要 7)。b5 の readiness が切れる(最大 24 DAA ≈ 66 分)までは、b5 を含む panel が残り得る(残り 4 seat で quorum 3) |

## 5. 配備後に見るもの

* ibm b1・.113 b6 の journal に `anchor duty: … (every REAL class refuses a new attempt: the short wait)` が出て、`waited N slots` の N が
  4 + stagger(両 bond とも stagger 6 なので 10 前後)であること(従来は 36)。
* claim の bind 待ち: `[palw-panel] event` の `ON_CHAIN` → `WAITING_RECEIPTS`。中央値 78 DAA から 30 DAA 前後へ。
* b0 の `holding: … has no panel room` の頻度と、`produced block` の数(12 h で 51 本が基準)。
* seat の未処理 duty(`seat schedule: N duties due`、今は 13〜17)が増え続けないこと。bind が早まった分、seat の待ちは伸びる。
* 5.104 の load・swap と、b2・b4 の replay 時間(今は中央値 710〜750 s、p90 約 10,000 s)。

## 6. 戻し

* binary: 問題の出た host から(複数なら 5.104 → .113 → ibm)、`fleet.env` の REV は **このリリースのまま** で
  `./install-<host>.sh upgrade-rollback`。戻る先は `247ade1f90b6` の launch script と binary(書き換えていない)。`rollback` は使わない(§6)。
* b5: `CONFIRM_TARGET_STOPPED=yes ./move-host.sh unretire 5`(5.104 で。marker と drop-in を消して旧 unit を起動)。
* fence を跨がないので、いつ戻しても chain は分かれない。
