# misakascan を新しい testnet-12 genesis に載せ替える手順（未実行）

> **2026-09-25 更新（R-core+、branch `rcore/release-prep`）— 先に §9 を読む。** 下の §0〜§8 は 09-23 の調査記録で、
> genesis `f6cc9576…` の記述は古い（その genesis は私設 t12 のもので、いまは `FORBIDDEN_GENESIS` に入っている）。

対象: `misakascan.com`（169.58.232.113 の `/var/www/misaka-explorer` と、その背後の indexer/REST）。
用意したもの: この `DEPLOY.md`、`deploy.sh`（未実行）、`app.js` / `index.html`（scratch 上の改訂版）、`probe.mjs`（読むだけの wRPC プローブ）。
.113 は 2026-09-23 15:28–15:45 CEST に **読むだけ**で調べた（cat / ls / systemctl / journalctl / ss / psql の select と `\d`）。何も書いていない。

## 0. 要点

- **新 genesis は `f6cc957686f7047d…3a963a30`**（`PALW_T12_GENESIS`、5a559459 の `consensus/core/src/config/genesis.rs`）。config ツリーは b38356fe と 5a559459 で同一なので、5.104 の私設 t12（b38356fe build、fp `41b4c74a…`）と**同じ genesis hash**。公開用 release の fingerprint はまだ無い（5a559459 以降の build が出す値を `EXPECT_FP` に入れる）。
- **misakascan.com はすでに私設 t12 を表示している。** 別 session が 14:47–15:08 CEST に、nginx・filler/REST・app.js を 5.104 の follower（`misaka-t12p-scan-tunnel` 経由の 127.0.0.1:36312/36314/36545）に向け替えた（詳細は §1）。今回の手順はその状態から出発する。
- 旧 indexer DB `kaspa_t11` は **t11（genesis `ad30b5cb…`）と旧 t12（`a8cabac4…`）が混ざっている**。MTP の collector は毎時 "WRONG CHAIN — kaspa_t11 has 3 chain roots" で取り込みを拒否している。**新しい DB `kaspa_t12` を作り、filler のカーソルを genesis に置いてから始める**（空の DB だと filler は tip から始めてしまう。私設用 `kaspa_t12p` は blue 22 から始まっていて genesis が入っていない）。
- 公開を止めるもの: (a) 新 genesis で動く公開 node がまだ無い（.113 の `misaka-t12-node` は旧 fp `c746f07c` で稼働中）、(b) `EXPECT_FP` が未確定、(c) faucet が testnet-11 のままで残高なし、(d) `PANEL_SEATS` にどの fleet を載せるか未決。§7・§8 を参照。

## 1. いま .113 で動いているもの（読むだけで確認した事実）

| 項目 | 値 |
|---|---|
| サイトのルート | `/var/www/misaka-explorer`（手で当てた `app.js` 313,534 B、`index.html` は `?v=t12p1`、`style.css?v=m15`、`readability.css?v=20260913-1`、`sha3.min.js?v=080`） |
| nginx vhost | `/etc/nginx/sites-enabled/misakascan`（**symlink ではなく通常ファイル**。`sites-available/misakascan` は 09-08 の古い版） |
| `/kaspa`, `/kaspa-seed` | upstream `misaka_json` → **127.0.0.1:36314**（私設 t12。元は 26314） |
| `/kaspa-hub` | **127.0.0.1:36314**（元は 28014 = ibm からの逆トンネル → ibm 127.0.0.1:26314。ただし ibm の t12 node の JSON は **26324** なので、元の配線でも hub は死んでいた） |
| `/evm` | **127.0.0.1:36545**（元は 8545） |
| `/transactions/`, `/addresses/`, `/info/` | 127.0.0.1:8011 = `kaspa-t11-rest-server`（gunicorn、`/opt/venv-rest`、`NETWORK_TYPE=testnet` → prefix `misakatest`） |
| `/mtp/` | 127.0.0.1:8790 = `misaka-mtp`（testnet-11 のポイント台帳） |
| `/faucet/` | ibm 169.58.39.220:8792 = `misaka-faucet`（**testnet-11**、`FAUCET_RPC=127.0.0.1:26313` だが ibm にその port は無い → `funded:false, balance null`） |
| `/pool/` | 127.0.0.1:8799 = `misaka-minerpool`（hosted testnet-11 slots。slot unit は全部 dead/failed） |
| indexer | `kaspa-t11-db-filler`（`/opt/explorer-stack/kaspa-db-filler`、`/opt/venv-filler`、gRPC を読む） |
| filler/REST の上書き | 両 unit に drop-in `t12p.conf`: `KASPAD_HOST_1=127.0.0.1:36312`、DB `kaspa_t12p`（base unit は 26312 / `kaspa_t11`） |
| .113 の公開 node | `misaka-t12-node`（`/root/t12/node.sh`、appdir `/root/.t12`、P2P 0.0.0.0:26311、gRPC 26312、Borsh 26313、JSON 26314、EVM 8545、fp `c746f07c…` = 旧 genesis `a8cabac4…`） |
| wallet.misakascan.com | `/kaspa` → 127.0.0.1:26314（**.113 のローカル node を直接**。explorer の向け替えとは別で、node が新 genesis に移れば自動で追従） |
| peer census | `misakascan-peer-census.timer`（2 分毎）。`CENSUS_LOG` 既定が `/root/.t11/misaka-testnet-11/logs/rusty-kaspa.log`（t11 node は停止済）→ `"network": null`。remedy 文言も t11 のフラグデー |
| MTP collector | root の crontab 毎時: `MTP_NETWORK=testnet-11 MTP_DB=kaspa_t11 MTP_RPC=127.0.0.1:26313`、pin `/etc/misaka-mtp/chain.pin`（GENESIS / DB_ANCHOR = t11 の `ad30b5cb…`） |
| `llm-jobs.json` | 4.4 MB、09-22 07:57 以降更新なし。中身は t11 の job（class `QWEN25-A16`、daa 1627 …）。書き手は .113 には無い |

PostgreSQL 16（127.0.0.1:5432、所有者 `kaspa`）:

| DB | サイズ | 中身 |
|---|---|---|
| `kaspa_t11` | 387 MB | blocks 11,759 行（blue 0–8,615）。genesis `ad30b5cb…`（t11）から。09-22 以降に旧 t12 の block（algo-8 が 1,599）が混入。checkpoint daa 264 |
| `kaspa_t12p` | 10 MB | 私設 t12。blocks 167 行、blue 22–128（**genesis 無し**、tip から開始） |
| `kaspa_t11_old_*` ×6 | 9–63 MB | 過去の再 genesis の退避（命名規則 `kaspa_t11_old_<genesis8>_<date>`） |

各 DB のテーブル: `blocks`, `transactions`, `transactions_inputs`, `transactions_outputs`, `tx_id_address_mapping`, `vars`（`vars` の key: `vspc_last_start_hash`, `filler_checkpoint_v2`, `last_id_counter_inputs`, `last_id_counter_outputs`）。

既存のバックアップ（別 session が作成）: `app.js.bak-before-t12p-20260923T150853`、`index.html.bak-before-t12p-20260923T150853`、`/root/misakascan.nginx.bak-before-t12p-20260923T144704`、`/root/patch_app_t12p.py`。

前回（旧 t12 `a8cabac4`）の再 genesis では explorer に何もしていない — `docs/testnet-12-regenesis-2026-09-23.md` に explorer の記述は無く、filler は `kaspa_t11` にそのまま t12 を書き足した。DB の回し方の前例は t11 5f（`docs/testnet11-relaunch-5f-genesis-card.md` の Explorer 節: 旧 DB を退避 → 新 DB → `vars.vspc_last_start_hash` に genesis を入れて block 0 から索引）。

## 2. app.js の中身（`app.js.orig` → scratch の `app.js`）

`app.js.orig` は 09-21 の live（`?v=ctx2m2`）と同一（sha256 `5d74b111…`）。

1. **t12 の表**（前回の run）: `PANEL_SEATS` を ix 0..7 の 8 席に（全席 `PALW-BASE-0` + `QWEN25-A16-8K`、2M を持つ席は無し）。`LLM_CLASSES` を floor `f1c5635c…` / 8K `ebf44d0a…` / 2M `74c67e63…` に。class id は 5a559459 の `class_manifest_const_v1.rs` と `t12_regenesis.rs` と一致し、私設 t12 の `getPalwModelRegistry` も同じ 3 行を返す（floor は `isBaseClass: true`）。
2. **route-matrix #8**: 概要の Consensus 枠は `getPalwNodeStatus` v3 の lane mix（`laneWindowBlocks` / `laneWorkBlocks` / `laneHeartbeatBlocks` / `laneAlarm`、serde camelCase で名前一致を確認）。v3 でない node では tip の algo 表示に戻る。floor は "not gated"（Models のカードと表、座席表の 3 か所）。"operator holds" を "operator notes: has the file / N noted / not proved" に。algo 8 / 9 / 10 のラベル（`pow_layer0.rs` の 8 heartbeat、9 execution-lane、10 round と一致）。
3. **今回の修正**（scratch の `app.js` のみ）:
   - `MODEL_DISPLAY["QWEN25-A16-8K"]` が無かった → Models ページで 8K 行が "This model has no name on the explorer yet." になり、いま live の t12p 版より退行していた。`Qwen 2.5 1.5B @8k`、~1.7 GB（5.104 上の `qwen25-1.5b-a16-8k.palwart` は 1,799,359,436 B）で追加。
   - `wColor["QWEN25-A16-8K"]` が無かった（予備色になっていた）→ live と同じ `#34d399`。
   - lane mix が一度入ると残り続けた → node が lane window 無しで応答したら（v3 より前の build、初回集計の前）`overlayStats.lane = null` にして tip 表示へ戻す。一時的な `__error` のときは直前の値を残す。
   - `node --check app.js` OK。表の自己整合（全 class に表示名がある・座席の holds が全部既知の class）も確認済み。

live（`t12p1`）と比べて scratch 版に無いのは**私設ロスター**（p0–p7 がすべて 5.104）だけ。scratch 版は前回の公開 fleet（ibm node0/node1、5.104 席 2–5 と 7、.113 席 6）を載せている。どちらが正しいかは §8 の 3。

## 3. 前提（満たすまで実行しない）

1. 新 genesis で動く node が explorer から見えること。既定は **.113 の `misaka-t12-node` 自体**（26312/26314/8545）— つまり fleet 側の切替（新 appdir、5a559459 以降の binary）が先。5.104 の follower を読み続けるなら `NODE_GRPC=127.0.0.1:36312 NODE_JSON=127.0.0.1:36314 NODE_EVM=127.0.0.1:36545`（tunnel は 5.104 の unit なので、公開用の follower として残すかは fleet 側の判断）。
2. `EXPECT_FP` = release build の `Consensus params fingerprint:` 行。
3. **私設 t12 と公開 t12 が同じ genesis hash を持つ**点に注意: genesis の一致では 2 本を区別できない。公開 chain を私設 chain の続きにするのか、同じ genesis から新しく始めるのかで、`kaspa_t12p` を使い回せるかが変わる（この手順は常に新しい `kaspa_t12` を作るので、どちらでも混ざらない）。
4. `PANEL_SEATS`（app.js 2951 行）を実際の launcher に合わせて直してから stage する。
5. `index.html` の運用者向け告知は testnet-11 の内容（fp `c3a5e91d…`、フェンス列、"Keep your appdir — the genesis did not move"、`docs/testnet11-6701-upgrade-announcement.md`）。再 genesis では**逆の指示**なので、`deploy.sh` は既定（`NOTICE=strip`）でこのブロックを外す。t12 の告知を載せるなら `index.html` を編集して `NOTICE=keep`。

## 4. 手順（`deploy.sh` の各段）

```sh
cd <scratchpad>/misakascan
EXPECT_FP=<64hex> ./deploy.sh preflight     # 読むだけ。node の fp、genesis の有無、DB 名の衝突、現在の配線
EXPECT_FP=<64hex> ./deploy.sh all           # 下の 1–6 を順に。途中で "DEPLOY" の入力を求める
```

段ごとに走らせる場合（同じ TS を `.last-deploy-ts` から引き継ぐ）:

1. **backup** — `app.js` / `index.html` / `llm-jobs.json` → `*.bak-before-t12g-<TS>`、vhost → `/root/misakascan.nginx.bak-before-t12g-<TS>`、両 unit の drop-in dir → `/root/misakascan-config-backups/units-before-t12g-<TS>.tgz`、`kaspa_t12p` → `…/kaspa_t12p-before-t12g-<TS>.dump`（`pg_dump -Fc`、約 10 MB）。`kaspa_t11` は触らないので dump しない。
2. **db** — `CREATE DATABASE kaspa_t12 OWNER kaspa;`、filler の model 4 つ（`models.Block/Transaction/TxAddrMapping/Variable`）で `create_all()`、`INSERT INTO vars(key,value) VALUES ('vspc_last_start_hash','<genesis>')`。テーブル集合が上の 6 つと一致しなければ止まる。`SQL_URI` は base unit から読んで DB 名だけ差し替える（パスワードを表示しない）。
3. **units** — filler を止め、両 unit の `t12p.conf` を `/root/misakascan-config-backups/<unit>.t12p.conf.moved-<TS>` へ**移し**（消さない）、`t12g.conf`（`KASPAD_HOST_1=$NODE_GRPC`、`SQL_URI=…/kaspa_t12`）を置き、`daemon-reload` → REST restart → filler start。`Start hash: <genesis>` が journal に出ることを確認。
4. **nginx** — `upstream misaka_json` の server、`location /kaspa-hub` と `location = /evm` の proxy_pass を、それぞれのブロック内で 1 行ずつだけ置換（1 回ちょうど当たらなければ止まる。wallet の vhost は触らない）。`nginx -t` 失敗なら即座にバックアップへ戻す。成功なら `systemctl reload nginx`。この置換は、faucet key を伏字にした vhost のコピーで 2 回（26314/8545 と 36314/36545）試して差分が 3 行だけなのを確認済み。
5. **files** — `stage-<TS>/` に `app.js` と `index.html` を複写、`node --check`、`/app.js?v=t12g-<TS>` を 1 か所だけ差し替え、告知ブロックを外し、空の `llm-jobs.json`（`{"rows":[],"fp_rows":[]}`）を作る。`/tmp/misakascan-t12g-<TS>/` に scp し、**app.js → index.html の順**で `install -m 0644`（逆順だと新しい `?v=` が一瞬 404）。開いたままのタブは app.js の ETag 監視（60 s）で自動再読込される。
6. **census**（任意、`CENSUS=1`）— `misakascan-peer-census` に `CENSUS_LOG=<t12 node の log>` の drop-in。
7. **verify** — 下の §5。

`deploy.sh` が触らないもの: kaspad の unit・appdir・binary、MTP の台帳と pin、faucet（ibm）、`kaspa_t11` / `kaspa_t12p` / `kaspa_t11_old_*`、wallet.misakascan.com。

## 5. 検証（`./deploy.sh verify`）

- `curl https://misakascan.com/` の `app.js?v=` が今回の buster と一致。`/app.js` の sha256 が stage したファイルと一致、`cache-control: public, max-age=300, must-revalidate`。
- REST: `/info/network` の `networkName` が `misaka-testnet-12`、`/info/health` が `isSynced:true`、`/info/stake-bonds` が 200。
- `/evm` に `eth_chainId` が返る。
- wRPC（`probe.mjs`、読むだけ）: `/kaspa`、`/kaspa-seed`、`/kaspa-hub` のそれぞれで `getBlockDagInfo.network = testnet-12`、`getBlock(<genesis>)` が見つかる、`getPalwNodeStatus.consensusParamsId = EXPECT_FP`。
- DB: `select count(*) from blocks where hash='<genesis>'` = 1（block 0 から索引している証拠）、行数と max blue score、filler の `Start hash`。
- 目視（任意）: Home の Consensus 枠（v3 node なら "work N · heartbeat M of the last W blocks"、heartbeat だけなら赤の警告）、Models（floor = "Checkers: every node · not gated"、8K/2M の ready / required）、LLM ページの座席表。

参考: 今日の私設 t12 に対して `probe.mjs` を当てた結果は network testnet-12、genesis found、fp `41b4c74a…`、`fenceSchedule [1000]`、class 4 行（2M Prefetching 0/7、`7d7e3d05…` Candidate 0/7（名前の無い後登録。5.104 に @2048 の artifact がある）、8K Prefetching 8/7、floor Active/base）。私設 node は v2 なので lane mix は出ず、tip 表示に戻る経路が動く。

## 6. ロールバック

```sh
./deploy.sh rollback            # 直近の TS
./deploy.sh rollback <TS>
```

`*.bak-before-t12g-<TS>` から app.js / index.html / llm-jobs.json を戻し、vhost を戻して `nginx -t` → reload、`t12g.conf` を消して `t12p.conf` を元の場所へ戻し、`daemon-reload` → REST / filler restart。つまり「私設 t12 を表示している今の状態」に戻る。`kaspa_t12` はそのまま残る（捨てるなら `ALTER DATABASE kaspa_t12 RENAME TO kaspa_t12_abandoned_<TS>;`）。

私設 t12 の表示より前（旧公開 t12 / t11 の配線）まで戻すなら、別 session のバックアップ `app.js.bak-before-t12p-20260923T150853` / `index.html.bak-before-t12p-…` / `/root/misakascan.nginx.bak-before-t12p-20260923T144704` を使い、両 unit の `t12p.conf` を消す（drop-in のコメントに書かれている手順）。

## 7. まだ testnet-11 や旧 genesis を指しているもの

サイトで読者に見える箇所（行番号は scratch の app.js）:

| 場所 | 内容 | 扱い |
|---|---|---|
| `index.html` 告知 | t11 の 2026-09-19 リリース、fp `c3a5e91d…`、フェンス列、"genesis did not move" | `deploy.sh` が外す（既定） |
| Faucet ページ（478, 497, 524 行） | "(testnet-11 — coins with no value)"、`docs/testnet11-node-operator.md`、"That is not a testnet-11 address" | **backend も t11**（ibm の `misaka-faucet`、`/opt/misaka-faucet.py`、RPC 26313 は ibm に存在しない、未入金）。t12 用の鍵と入金は運用者判断 |
| LLM ページの replay 手順（3212–3226 行） | `--network testnet-11`、QWEN36 の root `f4aad4fd…`、A16 v5 の変換 hash | 文面の更新が要る（今回は触っていない） |
| MTP ページ（4325–4343, 4479 行）と `/mtp/` | `misaka key gen --network testnet-11`、"Point a miner at testnet-11"… 台帳自体が t11 | MTP を t12 で続けるかは未決。collector の pin（GENESIS / DB_ANCHOR）と crontab の `MTP_DB=kaspa_t11` を新 chain に合わせない限り、fence が拒否し続ける（安全側） |
| `/info/stake-bonds` | `SEED_BOND_OUTPOINTS` に t11 の validator bond `e9e3ebba…:0` を固定 | 画面側が `getStakeBond` で確認して落とすので実害なし。REST のコードは今回触らない |
| peer census | t11 の log を読み、remedy 文言が t11 のフラグデー（891a1a14、DAA 2400） | `CENSUS=1` で log だけ t12 に向く。文言は `/usr/local/bin/misakascan-peer-census.py` 側 |
| `llm-jobs.json` | t11 の job 4.4 MB | block hash / claim 突合なので t12 には出ないが、LLM ページが 30 秒毎に取得する。`deploy.sh` が空の feed に置換（旧版はバックアップ） |
| `/kaspa-hub` の元配線 | 28014 → ibm 26314（ibm の t12 node は 26324） | 既定で `/kaspa` と同じ node に向ける。ibm 側の `vantage-tunnel.env` を直せば `HUB_JSON=127.0.0.1:28014` |
| `/pool/` | hosted testnet-11 slots（slot 全停止） | 未対応 |

コード内コメントの testnet-11 表記（313, 639, 1086, 1338, 1802, 2921, 3918, 3961 行）は読者には見えないので残した。

## 8. 未決事項（運用者へ）

1. **公開 t12 は私設 t12 の続きか、同じ genesis から新しく始めるのか。** どちらでも genesis hash は `f6cc9576…`。続きなら 5.104 の follower をそのまま読む選択肢もある（`NODE_*` を tunnel の port に）。
2. **explorer がどの node を読むか。** 既定は .113 の公開 node。これが新 genesis に移る時刻が deploy の開始条件。
3. **`PANEL_SEATS` はどの fleet か。** scratch は前回の公開配置（ibm/5.104/.113、全席 8K）、live は私設配置（p0–p7 全部 5.104）。
4. **`EXPECT_FP`**（5a559459 以降の release build の値）。
5. **faucet** を t12 で開くか（鍵・入金・`FAUCET_RPC`・文言）。開かないならページに注記を入れるか。
6. **MTP** を t12 で続けるか（pin の書き換えと台帳の退避は `chain.pin` のコメントどおり）。
7. index.html に t12 の告知を載せるか（載せないなら既定で告知ブロックを外す）。
8. 混在 DB `kaspa_t11` を規則どおり `kaspa_t11_old_ad30b5cb_<date>` へ改名するか（MTP の cron が参照しているので、MTP を決めてから）。


## 9. R-core+ での変更と未決（2026-09-25）

- **genesis と fp は deploy kit の `fleet.env` から読む**（`EXPECT_GENESIS` / `EXPECT_FP`、出荷 commit の probe で固定。
  `docs/t12-rcore-launch-checklist.md` §4）。`deploy.sh` の既定 genesis（`f6cc9576…`）は削除し、`FORBIDDEN_GENESIS` と
  `DRILL_GENESES` にある genesis は拒否する。a0af3c92 での暫定値は genesis `a27f8f44…`、fp `8a481023…`（出荷値ではない）。
- **`app.js` の `PANEL_BOND_TX`** を t12 固有の premine txid `5e0d5f1b…` にした（旧値は共有 sentinel `6d697361…` で、t12 の bond
  outpoint と一致しないため座席表の bond 列と receipt の突合が外れていた）。出荷 commit の probe の `PREMINE_TXID` と照合してから stage する。
- 2M は公開時点で閉じる（ADR-0152 §8.3 item 7、O-11）。`LLM_CLASSES` の 2M 行の説明にそう書いた。class id 3 本は a0af3c92 でも同じ
  （`palw_t12_genesis_held_class_ids_v1` と `PALW_T12_RCORE_CONSERVATIVE_CLASSES`）。
- `NODE_LOG` の既定を deploy kit の b6 appdir（`/root/.t12r-b6/…`）にした。
- **未決（運用者）: explorer の配線が 2 系統ある。** deploy kit の `install-113.sh explorer-apply`（nginx 3 行、filler/REST の
  `zz-t12r.conf`、DB `kaspa_t12r`、cursor は seed しない）と、この `deploy.sh`（`t12g.conf`、DB `kaspa_t12`、cursor を genesis に
  seed、app.js / index.html の stage と verify）。同じ unit と vhost を触るので **どちらか一方**にする。推奨は `deploy.sh` 一本
  （genesis から索引し、files と verify も持つ）で、`explorer-apply` は使わない。決まるまで §4 の `all` も `explorer-apply` も
  実行しない。どちらも **公開サイトを書き換えるので実行前にユーザーの確認**が要る。
- **R-core+ の表示が未対応**: vesting 行（`getPalwVesting`、op 199）、DA session、reporter の commit–reveal、stake 加重の panel 抽選は
  explorer に出ていない。公開後の観測 O-2 / O-5 は `misaka palw vesting` と analyzer で読む。explorer への追加は公開後の作業。
