# testnet-12 DAA 750 post-launch release — rollout checklist

2026-09-26 作成（`rcore/int-4`）。**この文書のどの手順もまだ実行していない。** 読み取りで確かめたのは次の 3 つだけ:
.113 の `/etc/misaka-mtp/chain.pin`（`cat` のみ）、IBD checkpoint の候補（explorer の node と ibm の node への `getBlock`）、
公開 node の DAA。

> **暫定値の注意（最重要）**: 13 本目の fence `palw_lane_accept_parents_first`（branch `rcore/f1-lane-accept-order`、
> 未 merge）が同じ DAA 750 のリリースに入る。merge して 750 に武装し `scripts/t12-repin.sh --apply --shipping` をやり直すと、
> **下の fingerprint と schedule id は変わる**（identity・genesis・premine は変わらない見込み — 将来の高さの fence は identity に
> 入らない）。§2 の値はすべて「13 本目の fence が入ったら再 pin する」値として扱い、**再 pin 前の値で release build・fleet.env・
> 告知をしない**。

## 1. このリリースに入っているもの

| 種類 | 中身 | consensus か |
|---|---|---|
| post-launch fence ×12（DAA 750、`PALW_T12_POST_LAUNCH_FENCE_DAA`） | `palw_registry_resilience`・`palw_panel_seed_execution`・`palw_heartbeat_transparent_same_chain`・`palw_reorg_strict_economic_win`・`palw_bond_maturity_early`・`palw_model_sink_bound`・`palw_operator_anchor`・`palw_final_lock_full_collateral`・`palw_final_lock_life`・`palw_anchor_at_ceiling`・`palw_slashing_evidence_utxo_genuine`・`palw_pruning_proof_strict_economic_win` | **はい**（750 から） |
| （予定）13 本目 | `palw_lane_accept_parents_first` — 別 agent が dormant で実装中、merge 後に 750 へ | はい（750 から） |
| IBD checkpoint | `PALW_T12_IBD_CHECKPOINTS` = DAA 100 / 200 / 300 の chain block（§7） | いいえ（node policy） |
| IBD 順序の修正 | `426c32c05`（int-4 では `a067f490d`）: round lane の block を parents-first で渡す。DAA 316 以降、新規 node の IBD と、lane をまたいで IBD に落ちた node が `missing parents` で止まる不具合 | いいえ（node only） |
| lifecycle UX | `rcore/n7-lifecycle-ux`（T12-030/046/049/058）: node・CLI・RPC・docs | いいえ |

**IBD 修正について**: 一度でも再同期する node（新規参加・datadir を消した node・長く止まっていた node）は、この修正を含む build
でなければ DAA 316 以降の lane を越えられない。fleet の更新は **必ず `upgrade`（in-place、appdir 維持）で行い、`switch` は使わない**
（`switch` は regenesis: appdir を退避して genesis から同期し直すので、この修正の有無に関係なく危険で、round 署名記録も一緒に動く）。

## 2. identity（旧 → 新）— 13 本目の fence が入ったら再 pin する暫定値

`scripts/t12-repin.sh --apply --shipping`（`a1b1c4491`）が build から計算した値。確認の dry run: 253 ok・drift なし・未登録 literal なし。

| 項目 | launch release（`0e8ec984e`、現在の fleet） | このリリース（**暫定**） |
|---|---|---|
| `consensus_params_id`（fingerprint、`EXPECT_FP`） | `b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f` | `1274ac128a77482840d8785bf86487d7632cbfa6751140d1091d78cf2d4657f7` ※再 pin で変わる |
| `consensus_schedule_id` | `93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd` | `ae8cc4b7615fbbb7fa3cbf809bb8b867740afe98731c2f9c3bd8ab768095a300` ※再 pin で変わる |
| fence schedule（起動ログ） | `1000` | `750, 1000` |
| `consensus_identity_id` | `5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5` | 同じ（変わらない） |
| genesis（`EXPECT_GENESIS`） | `a27f8f44…a8ca1f23` | 同じ |
| premine txid（`PREMINE_TXID`） | `5e0d5f1b…e55e2669` | 同じ |
| rule manifest digest | `9def81a1…` | 同じ |

他の network（testnet-11 `bd633ce9…`、devnet `7a27f341…`、mainnet `eb866c61…`、testnet-10 `0d9cf361…`、simnet `63238ba1…`）の id は
動いていない（repin の harvest で確認）。

## 3. 時刻の目安

2026-09-26 19:24 JST に DAA 457、約 142 s/DAA。この速度のままなら **DAA 650 ≈ 9/27 03:00 JST**（fleet 全体がこのリリースで
動いていなければならない目安）、**DAA 750 ≈ 9/27 07:00 JST**。速度は変わるので、作業の前に `getBlockDagInfo.virtualDaaScore` を
読み直すこと。**DAA 750 を過ぎてから旧 binary に戻すことはできない**（§6）。

## 4. node の外で fingerprint / fork id / schedule id を pin・表示しているもの

| 対象 | 何を pin / 表示しているか | 切替時にすること |
|---|---|---|
| **DNS seeder**（4 台、`misaka-dnsseeder-t12` `1174b965…`、`SEEDER_SHA256=KEEP`） | **何も pin しない。** `--anchors-only` で、operator が指定した anchor（169.58.232.113、169.58.39.220）へ `:26311` の TCP 接続ができるかだけを見る。genesis・params id・fork id・version を読まず、P2P handshake もしない（`misaka-dnsseeder/src/main.rs` `refresh_verified`）。address manager の peer は anchors-only では配らない | **何もしない。** KEEP の seeder は upgrade 後も同じ IP・port の anchor を配り続ける。旧 version の node は anchor でない限りもともと配られない（fleet 外の node は新旧どちらも配られない）。切替後に `seeders/40-verify.sh` と `CONFIRM=yes seeders/60-join-check.sh <新 release の kaspad>`（`EXPECT_FP` は fleet.env の新しい値を読む） |
| **explorer**（misakascan、.113） | `deploy.sh verify` が `/kaspa`・`/kaspa-seed`・`/kaspa-hub` の `getPalwNodeStatus.consensusParamsId` を `EXPECT_FP`（fleet.env）と比べる。`index.html` の告知 banner が fp・fence schedule・schedule id を表示（この commit で新しい値に更新済み — **再 pin 後にもう一度**。registry に登録したので `t12-repin.sh` が書き換える） | .113 の node がこのリリースになった後で `./deploy.sh files`（banner）→ `./deploy.sh verify`。それまでは旧 banner のまま（node が出さない fp を表示しない）。`app.js` に fp は無い |
| **MTP collector**（.113 `/etc/misaka-mtp/chain.pin`） | `NETWORK=testnet-12`・`DB=kaspa_t12`・`RPC=127.0.0.1:26313`・`GENESIS=a27f8f44…`・`DB_ANCHOR_DAA=0`／`DB_ANCHOR_HASH=a27f8f44…`。`mtp-chain-fence.sh` は node の pruning point を `GENESIS` と比べるだけで、**fingerprint は見ない**（2026-09-26 20:00 JST 読み取りで確認） | **何もしない**（genesis 不変）。pruning point が genesis を離れる DAA ~75k までは今の pin のままでよい |
| **deploy kit**（`fleet.env`） | `EXPECT_FP`（stage の IDENTITY 照合、`upgrade` の再起動後 gate、`check`、`t12check.py --expect-fp`）。`t12check.py` 自体は値を持たない | `REV`・`KASPAD_SHA256`・`MISAKA_SHA256`・`PALW_CLASS_SHA256` を新しい build の値に、`EXPECT_FP` を**再 pin 後の**新しい fp に、**`UPGRADE_FROM_FP=b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f`**（今 fleet が出している fp）。`EXPECT_GENESIS`・`PREMINE_TXID`・`SEEDER_SHA256=KEEP` はそのまま |
| **`contrib/t12-monitor`**（panel-bias monitor） | `T12_FP`（`--expect-fp` の既定値）— 新しい値に更新済み（registry 登録済み）。fp が違うと DEGRADED になる | fleet 切替後、`--fence-daa 750` を付けて走らせる（母集団規則の切替。付けないと fence 後の anchor 規則が合わず DEGRADED）。切替前に走らせると fp 不一致で DEGRADED になる（仕様） |
| **main の文書**（`origin/main`、切替と同時に更新する） | `README.md`（表の fingerprint と schedule id）、`release.json`（`consensus_params_fingerprint`・`consensus_schedule_id`）、`docs/testnet12-join-mining.md`（起動ログの例 2 行）、`docs/t12-rcore-launch-checklist.md` §3、`contrib/t12-deploy-kit/PLAN.md` §4、`contrib/misakascan-t12/index.html`・`DEPLOY.md`、wiki: `docs/wiki/Home.md`・`Operations-Notes.md`・`Quick-Start.md`・`Testnet-12-Verification-Participation-JA.md`、wiki staging の `docs/wiki/wiki-t12-sync.patch` | int-4 を main に反映するとき一緒に新しい値へ（int-4 側の README・join guide・checklist・PLAN・DEPLOY・index.html は更新済み）。`docs/t12-launch-2026-09-25.md` は launch の記録なので書き換えず、「DAA 750 の post-launch release で fp が変わった」注記を足す。告知は fleet 切替後 |

### fork id / handshake の前提（なぜ rolling upgrade ができるか）
handshake が一致を求めるのは `consensus_identity_id`（変わらない）と fork id。fork id は fence の高さを見て、**DAA 750 より前は
新旧 build が互いを受け入れる**（`an_armed_build_below_its_fence_handshakes_with_the_shipped_build`）。params id の違いは
`flow_context` の警告になるだけ。したがって 1 台ずつ更新しても、750 前なら新旧の node はつながったまま。

## 5. kit の手順（Mac → 各 host、1 host ずつ）

**前提**: この文書の kit（`lib.sh` の `UPGRADE_FROM_FP` 対応）を配ってから行う。対応前の `upgrade` は、稼働中の node の fp が
`EXPECT_FP` と違うことを理由に**何も止めずに拒否する**（fp が変わる fence release を想定していなかった）。

0. **13 本目の fence を merge → 750 に武装 → `t12-repin.sh --apply --shipping` → suites** を通した int-4 の commit を確定する
   （この文書の §2 の値はそこで置き換わる）。
1. Mac: `./build-release-local.sh <その commit>`（`PROBE_IMAGE=rust:latest NATIVE_PROBE=1`）。出力の `IDENTITY` の `EXPECT_FP` が
   再 pin 後の checklist §3 と一致し、`EXPECT_GENESIS`・`PREMINE_TXID` が今の fleet.env と一致すること。
2. Mac: `fleet.env` を更新 — `REV`・`KASPAD_SHA256`・`MISAKA_SHA256`・`PALW_CLASS_SHA256`・`EXPECT_FP`（新）・
   `UPGRADE_FROM_FP=b8564b88…`（旧、全 64 桁）。
3. Mac: `./distribute-from-mac.sh kit` → `./distribute-from-mac.sh binaries`【確認】（既定の経路で 5.104・.113・ibm へ。build の
   IDENTITY が fleet.env と違えば送る前に止まる）→ `./check-fleet.sh`（この時点では全 node が旧 fp なので fp は MISMATCH と出る。
   synced・peer・daa が進んでいることだけを見る）。
4. 各 host: `./install-<host>.sh stage` → `DRY_RUN=1 ./install-<host>.sh upgrade`。3 台とも先に済ませてよい（稼働中の service に
   触れない）。DRY_RUN が `UPGRADE_FROM_FP` の fp で baseline を読み、最後まで通ること。launch script の `ARGS` に差分が出たら
   内容を確かめ、意図したものなら `UPGRADE_ARGS_CHANGE_OK=1`。
5. **.113【確認】**: `./install-113.sh upgrade`（b6）→ `./install-113.sh check`、Mac で `CHECK_REGISTRY=1 ./check-fleet.sh` を
   2〜3 分あけて 2 回: b6 が新 fp・synced・peer あり・daa が進む。misakascan の filler・REST が再接続して進む。
6. **ibm【確認】**: `./install-ibm.sh upgrade`（b0 → b1）→ 同じ確認 ×2。8k の `readySeatsNow ≥ 7`。
7. **5.104【確認】**: `./install-5104.sh upgrade`（b2 → b3 → b4 → b5 → b7）→ `CHECK_REGISTRY=1 ./check-fleet.sh` ×2。
8. **host 間の sink 一致**: 全 node の `getBlockDagInfo` の `sink`・`virtualDaaScore` が同じ（数 block の揺れは待って読み直す）。
   例: Mac から `node wrpc-call.mjs wss://misakascan.com/kaspa getBlockDagInfo '{}'` と ibm（tunnel `ws://127.0.0.1:36314`）で比べ、
   各 host の `check` の daa と合わせる。起動ログに `Consensus fence schedule: 750, 1000 (schedule id <新>)` と
   `IBD checkpoints (3): 100:…, 200:…, 300:…` が出て、checkpoint 自己検査の ERROR が無いこと。
9. 公開側: explorer（§4）、seeder の `40-verify.sh`・`60-join-check.sh`、monitor を `--fence-daa 750` で、main の文書と告知。

PLAN.md §15 の表は ibm → .113 → 5.104 の順だが、今回は .113 → ibm → 5.104 で行う（どちらの順でも、各 host の `upgrade` は
`UPGRADE_REQUIRE_UP` で他 host の node が生きていることを node の停止ごとに確かめる）。

## 6. 戻し

- **DAA 750 より前**: 問題の出た host から（複数なら 5.104 → ibm → .113）、fleet.env の `REV` はこのリリースのまま
  `./install-<host>.sh upgrade-rollback`。launch release の binary と unit に戻り、appdir はそのまま。`rollback` は使わない（regenesis）。
- **DAA 750 以降**: 旧 binary に戻した node は fence 後の block を旧規則で検証するので、fleet から外れる（§8 と同じ状態）。戻すなら
  fleet 全体を 750 前に戻すしかない。750 を過ぎた後の不具合は、新しい node-only 修正を `upgrade` で入れて直す。

## 7. IBD checkpoint（node policy、consensus ではない）

`PALW_T12_IBD_CHECKPOINTS`（`consensus/core/src/config/ibd_checkpoint.rs`）:

| DAA | block | blue score | 確認 |
|---|---|---|---|
| 100 | `c8b193a22e8f3f60f9955374842ccd8382849a0a896dc68f547f0e327e5b055f6086b1133b034c7444ec847cf4ab89e129e43e12e550f5f2de3084dde082fc67` | 438 | explorer の node・ibm の node の両方で `getBlock` → `isChainBlock=true`、`daaScore=100` |
| 200 | `eb396297171cd59d8ecfd651ad2919dbe45589744a3e56e59de8f1ef2fe8ad8620b8a034ff497ab6e9f0c3774395a56a841be43a274457ff8865a3f6cd84f1ab` | 879 | 同上 |
| 300 | `86426d61447712f8b152ef08e8c35178664acddcc854113a770bed7a16b08517a349651fe768c25c150a6c449f869c60f769a7f13244869f606098c9026e0913` | 1,172 | 同上 |

読んだ時点の sink は DAA 458・blue score 2,130（最も浅い DAA 300 でも 958 blue score 下、finality depth 600 より深い）。
同じ DAA score の chain block が複数ある（attempt lane は DAA を進めない）ので、各 score の**最初の** chain block を選んだ。
**更新期限**: honest な pruning point がこれらを pruning proof の level-0 window 以上通り過ぎると、新規 node が honest な proof を
拒否する。t12 の pruning point は DAA ~75k まで genesis なので当面は安全だが、その前に新しい entry へ差し替えること。

## 8. 更新しない外部 node が DAA 750 で経験すること

- **DAA 750 まで**: 何も起きない。更新済みの fleet と peer のまま同期する（identity 同じ、fork id は 750 前なら互いに受け入れる）。
  params id の違いは双方のログに警告として出る。
- **DAA 750 から**: 更新済みの node は、自分の DAA が 750 を過ぎると、fence 750 を持たない peer との handshake を fork id で拒否する
  （`DisagreePastFence`、750 を名指し）。旧 node 側の判定は相手が何を次の fence と名乗るか次第だが、更新済み側が拒否するので
  接続は成立しない。つまり **handshake 拒否で fleet から切り離される**（黙って別 chain に乗るのではなく、名前付きの拒否）。
- 同時に、750 以降の block は新規則（panel seed・operator anchor・lock・sink 等）で fold される。どの block で最初に分かれるかは
  どの規則が最初に効くか次第だが、PALW の state commitment が旧規則の計算と合わなくなった時点で、旧 node はその block を受け入れ
  られない。旧 node は止まる（自分で掘らない限り）か、旧 node 同士で別の枝を伸ばす。
- **戻り方**: 新しい release に更新すれば、同じ genesis・同じ identity なので datadir をそのまま使える（`upgrade` と同じ）。
  750 以降に旧 node が自分で block を掘って別の枝に乗っていた場合は、更新後も自分の枝を持ち続けることがあるので、datadir を
  消して再同期する（その場合は IBD 修正入りの build が必須）。
- DNS seeder は anchor（更新済みの fleet）だけを配るので、旧 node が DNS で新たにつながる先も更新済みの fleet で、750 以降は拒否される。

## 9. 既知の未解決・注意

- 13 本目の fence（`palw_lane_accept_parents_first`）が未 merge — §2 の値は暫定。
- `scripts/t12-repin.sh --selftest` が 17/22: selftest の期待値が cap / resilience lane の pin 追加より古い（このリリースとは無関係）。
- drill の ruleset もこのリリースの高さ（750）を持つ（drill は公開 ruleset と同じ組立てを通る設計）。`--palw-drill-fence-at` は
  それを低い高さへ動かす（MOVED と表示される）。
