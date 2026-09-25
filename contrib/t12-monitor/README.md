# testnet-12 panel-bias monitor（panel-seed stopgap の lane C）

作成: 2026-09-26（branch `rcore/panel-bias-monitor`、`rcore/int-3` の tip 基準）。**どこにも install していない。**
公開 t12（`wss://misakascan.com/kaspa`）に対して手で走らせ、動くことだけを確かめた（02:10 JST、tip DAA 70:
bound 139 件すべて anchor 一致、chain は 2 page で sink まで、exit 0）。

## 何をするか

公開 t12 を JSON wRPC で **読むだけ**の monitor。送るのは read call だけで、node の状態は何も変えない
（`getBlockDagInfo`、`getPalwNodeStatus`、`getPalwModelRegistry`、`getPalwPanelSeats`、`getPalwClaims`、
`getPalwPanelAssignments`、`getVirtualChainFromBlock`、`getBlocks`、`getBlock`）。stdlib だけで動く 1 ファイル
（`panel_bias.py`、Python 3.8 以上）。

claim ごとに次を記録する。記録は state file に残るので、claim が node の state から retire した後も証拠が積み上がる。

- claim id、class、producer bond
- anchor block と、その producer bond
- panel の seat。genesis の 8 bond は host 名で表示し、外部 bond はそのまま表示する
- seat ごとの receipt 状態（`valid` / `pending` / `none`）
- licence / void の結果（phase と voidReason）
- redraw されて 2 つ目の panel を bind した claim は、1 つ目の panel を別 record（`<claim>#<boundDaa>`）として残す

## anchor block の決め方

anchor は、`bind_base + anchor_delay` 以上の DAA を持つ、selected chain 上で最初の attempt block（algo 6/9）である。
`bind_base` は `reboundDaa`、無ければ `acceptedDaa`。根拠は `processor.rs` の `palw_v2_anchor_fact_of_candidate` と
`palw_block_may_anchor_a_panel_v1`。panel はその block で bind する（SW-8）。

- **bound claim の anchor は `boundDaa` から読む**（その DAA で最初の、anchor になれる attempt block）。そのうえで slot の規則が
  そこへ行き着くかを毎回確かめ、行き着かなければ `mismatch` として DEGRADED にする。
- **redraw された claim**（`reboundDaa` あり）は `reboundDaa` から確かめる。再 bind する前に見えている panel は 1 つ目の panel なので、
  `acceptedDaa` から確かめる。
- **registry-resilience の再 anchor**（`rcore/f1-registry-resilience`、`--resilience-from`）: 有能な panel が無い claim は anchor
  block に re-base され、`anchor_delay` 先で再試行する。bind した時点で `reboundDaa` は None に戻る。monitor はこの再試行を
  （最大 29 回）追って確かめる。fence が武装されていない間にこの形が出れば、規則が崩れたとして DEGRADED にする。
- **operator-anchor stopgap**（lane A、`--stopgap-from`）: その高さ以降は genesis bond の attempt だけが anchor になれる
  （block 自身の DAA で判定）。genesis の attempt が無い DAA で bind した claim は **ALERT**（consensus の失敗）。
- anchor block の producer は、その header の PAV2 envelope に入っている executor bond から読む。読めなければ DEGRADED。
- lane A の seed は「slot 以降で最も早い operator attempt」から読まれ、anchor block とは別の block のことがある（WIP）。
  この monitor は偏りを anchor block の producer に帰属させる。lane A 以降はどちらも operator なので、帰属先の operator が
  ずれることはあり得る。

## chain の読み方

- `getVirtualChainFromBlock` は 1 回に最大 1,800 chain block（10 × mergeset_size_limit）しか返さない。monitor は
  **空の page が返るまで（= sink まで）page を読み続ける**。`--max-chain-pages`（既定 60 ≈ 108,000 block）で止まったら
  DEGRADED。
- 読み始めは「まだ解決していない anchor のうち最も低い slot」より下の checkpoint。**解決済み（`ok` 等）の claim は、tip から
  `--recheck-daa`（既定 30、reorg 用）より古ければ読み始めを引き戻さない**。checkpoint は block 自身の DAA で保存する。
- bound claim の anchor が読んだ chain より先にある（`pending`）なら DEGRADED。
- tip から `--max-scan-daa`（既定 2,000）より古い slot は読みに行かない（`too-old`）。初回の run では注記だけ、2 回目以降に
  bound claim が該当すれば（停止していた等）DEGRADED。

## claim 一覧の上限

`getPalwClaims` は bond × role ごとに **新しい順に 500 行**までしか返さない。t12 では claim が終わってから 3,000 DAA 残るので、
genesis の seat 一覧は DAA 250〜335 あたりから常に上限に当たる。**これは通常の状態として扱う。**

- DEGRADED にするのは、返った 500 行の最も古い acceptedDaa が、前回の run が読んだ高さ（− `--recheck-daa`）に届かないとき
  （= 間の claim を取りこぼした可能性がある）だけ。hourly なら 1 時間に 1 一覧あたり ~60 行なので、~9 時間以上止まらない限り起きない。
- 一覧から外れただけの claim は `gone` にしない。`gone` は、その claim が入るはずの一覧を acceptedDaa まで丸ごと読めたのに
  無かったときだけ。

## 統計

**帰無仮説 H0 は「panel の draw は anchor block を誰が出したかに依存しない」。** alert はこの比較だけから出す。

- **relative cell 検定（alert）。** (anchor producer A, seat bond b) ごとに、A が anchor した claim で b が座った回数を、
  **同じ時間帯に他の producer が anchor した claim** と比べる。層は（class × executor × panel の大きさ × anchor DAA を
  `--stratum-daa`（25）で区切った帯）。層ごとの seat 合計を条件にすると、回数は正確に超幾何の和になる（層別 Fisher の正確検定）。
  - 両側 p 値。安いうちは畳み込みで正確に、それを超えると連続補正 + 歪度補正（Cornish–Fisher）付きの正規近似。
    正規近似は over 側（Sybil を座らせる grind の側）で p を大きめに出す（誤警報を増やさない側）ことを unit test で確かめている。
  - Holm–Bonferroni で、全 window の全 cell を family-wise `--alpha` に抑える。
- **relative omnibus 検定（alert）。** A の seat 回数ベクトル全体を他の producer と比べる一般化 Cochran–Mantel–Haenszel
  統計量（Q = D′V⁺D、χ²、自由度 = rank V）。producer と window にわたって Bonferroni。
- **producer が 2 つだけのとき**、A 対 B と B 対 A は同じ仮説なので、両方の名前で表示するが補正では 1 回と数える。
  どちらが外れているかは下の model 側の数字で読む（alert 行に両方出る。model から外れている側が先に並ぶ）。
- **window。** 全記録に加え、直近 `--windows` DAA（既定 50, 100, 200, 400, 800。幾何級数なので、振る舞いが変わった時点に
  最も近い window が 2 倍以内に入る）に anchor した claim でも同じ検定をする。前の window と同じ claim しか含まない window は
  検定しない。
- **false alarm 率。** 既定の `--alpha 1e-4` で、1 run あたり ≤ 2×10⁻⁴（cell と omnibus の 2 family）。hourly に回すと
  union bound で ≤ 0.5%/日。連続する run はデータを共有するので、実際はこれより低い。synthetic の較正（unit test）:
  omnibus の棄却率は名目 0.05 に対して 0.02〜0.09 の範囲、2 family・3 window・α=0.05 で alarm 率 ≤ 0.14。
  公平な 56,000 claim（保持上限の規模）で alert 0、解析 5 秒。
- **母集団全体に効く効果は alert にならない。** Valid-lock で飽和した seat、失効した readiness row、model が見落とした bond などは、
  同じ時間帯の全 producer に等しく効くので、producer 同士の比較には出ない（unit test: g2 を全 anchor で 25% 除外、
  1,500 claim → exit 0、population note だけ）。

**model note（alert ではない）。** 各 claim の母集団を stake-weighted successive sampling（exponential race、ADR-0152 SW-2/SW-3、
posted collateral を MSK 単位で 1,000,000 MSK cap）でモデル化し、正確な Poisson-binomial の cell 検定と Rao–Scott 補正の omnibus を
出す。population note として表示し、alert 行では「model から外れているのはどちらか」を示す。

- 母集団は claim の bind を最初に観測した run で作る（executor は除く、base class は宣言した bond、それ以外は anchor 時点で
  readiness が有効だった bond（`readinessProvedDaa..readinessExpiresDaa`）、Active、seat floor 130,000 MSK 以上、非 genesis は
  maturity）。
- readiness row が anchor より後に再証明されていた claim は母集団が不確か（`popUncertain`）として model note から外す
  （relative 検定には入る）。2026-09-26 02:10 の live run では 8k の 3 件がこれに当たった。

**検出力（synthetic、t12 の ~2.8 claim/DAA、grind する producer が claim の半分を anchor、特定の 2 seat を毎回座らせる、
α=1e-4、既定 window）。** 各 12 試行。

| grind の前の正直な履歴 | grind 25 DAA | 40 DAA | 60 DAA |
|---|---|---|---|
| なし | 4/12 | 12/12 | 12/12 |
| 700 DAA（~2,000 claim） | 1/12 | 7/12 | 12/12 |
| 5,000 DAA（~14,000 claim） | 0/12 | 4/12 | 12/12 |

数週間動かし続けた後でも、grind が始まってから ~60 DAA（2.6 分/DAA で ~2.6 時間）で捕まる。全記録だけの検定では、
2,000 claim の履歴の後の 150 claim の grind は 0/10 だった（検証 lane の測定）。

## alert の種類（exit 1）

- **bias** — relative cell または relative omnibus が有意なとき。両側なので、特定 seat を**外す**側の偏りも出る。
  行には同じ window の model 側の observed/expected と p も付く。
- **external seat** — 非 genesis bond が panel に**初めて**座ったとき。bond ごとに 1 回だけ。以後の過剰な座り方は
  relative cell が捕まえる。`--realert` で再掲する。
- **external anchor past the stopgap** — `--stopgap-from` 以降に非 genesis の producer が anchor した claim（claim ごとに 1 回）。
  それより前の external anchor は report と summary に数えるだけで alert にしない。

**DEGRADED**（exit 2）: RPC 失敗、chain が sink まで読めない、bound claim の anchor が読んだ範囲外、anchor 規則の不一致、
anchor producer が読めない、claim 一覧の取りこぼし（上記）、consensus params fp が出荷値（`b8564b88…`）と違う。
alert は DEGRADED より優先する。

## post-launch fence を武装したら

fp が変わるので、全 run が DEGRADED になる（わざと）。母集団規則を見直してから、共通の武装高さ H を渡す:

```sh
python3 contrib/t12-monitor/panel_bias.py --fence-daa H --expect-fp <新 fp>
```

`--fence-daa H` は `--stopgap-from H` と `--resilience-from H` をまとめて設定する（個別にも渡せる）。

## 出力

人間向けの report を stdout に出し、最後の 1 行に machine 用の summary を出す。

```
PANEL_BIAS {"status":"OK","time":...,"tip":70,"fp":"b8564b888e55bb5f","claims":191,"bound":139,"tested":139,"comparable":135,
            "anchors":{"g1":97,"g6":42},"windows":["all"],"ext_seat":0,"ext_seat_bonds":0,"ext_anchor":0,
            "ext_anchor_post_stopgap":0,"bias_cells":0,"min_cell_p":0.167,"min_omnibus_p":0.426,"model_notes":0,
            "unresolved":{},"alpha":0.0001,"alerts":[],"degraded":[]}
```

- 実際の出力は 1 行。`--quiet` を付けるとこの行だけを出す。`--json FILE` で全 record と全検定結果を書き出す。
- exit code: 0 = OK、1 = ALERT、2 = DEGRADED / 評価できない。monitor 自身が落ちても 2。

report の中身: 最近の panel、producer 同士の比較表（`comp/claims` = 他の producer と層を共有した claim 数 / 全 claim 数、
`!` = いずれかの window で有意）、model との比較表（`~` = model note）、host 別、window ごとの検定結果、external bond、alert。

## 走らせ方

```sh
# 1 回（Mac から公開 endpoint へ。~26 read call、~30 s）
python3 contrib/t12-monitor/panel_bias.py
# state は ~/.t12-panel-bias/state.json（--state で変える、'' で保存しない）

# 保存済み state だけで再解析（network 無し）
python3 contrib/t12-monitor/panel_bias.py --offline

# test（network 無し、~25 s）
python3 -m unittest -v contrib/t12-monitor/test_panel_bias.py
```

主な option:

| option | 意味 |
|---|---|
| `--alpha` | 検定 family ごとの family-wise 誤警報率（既定 1e-4） |
| `--windows` | 直近の window（DAA、カンマ区切り、既定 `50,100,200,400,800`、`''` で無し） |
| `--stratum-daa` | producer 同士を比べる時間帯の幅（既定 25 DAA） |
| `--fence-daa` / `--stopgap-from` / `--resilience-from` | post-launch fence の武装高さ（武装後だけ渡す） |
| `--retain-daa` | node が見せなくなった claim を何 DAA 分保持するか（既定 20,000 ≈ 4 週間。state は約 0.6 KB/claim） |
| `--recheck-daa` | tip からこの範囲の anchor は毎回解き直す（既定 30、reorg 用） |
| `--max-scan-daa` | tip からこれより古くは chain を読まない（既定 2,000） |
| `--realert` | external seat / anchor の alert を毎回出す |
| `--roster FILE` | genesis bond の名前（`{"0": {"host": "ibm", "name": "..."}}`）を差し替える |
| `--url` | endpoint（`ws://` / `wss://`） |

### hourly（Mac の cron）— install はしていない、例だけ

`crontab -e` に次を足す（1 行、paths は checkout に合わせる）:

```cron
7 * * * * mkdir -p $HOME/.t12-panel-bias && /usr/bin/python3 /Users/wata/Downloads/MISAKA-wt-b/wt-monitor/contrib/t12-monitor/panel_bias.py > $HOME/.t12-panel-bias/last-report.txt 2>&1; rc=$?; tail -1 $HOME/.t12-panel-bias/last-report.txt >> $HOME/.t12-panel-bias/summary.log; [ $rc -eq 0 ] || osascript -e "display notification \"exit $rc - see ~/.t12-panel-bias/last-report.txt\" with title \"t12 panel-bias\""
```

- `summary.log` には 1 時間に 1 行の JSON がたまる。
- 0 以外の exit で通知が出る。1 は alert、2 は degraded。通常の t12 運用（一覧の 500 行上限、正当な外部 seat）では
  exit 0 のままになるように作ってある。
- macOS の cron が `~/Downloads` を読めない場合は、`/usr/sbin/cron` に Full Disk Access を与えるか、file を別の場所に置く。

### hourly（.113 の cron）— install はしていない、例だけ

1 ファイルなので、置くのは `panel_bias.py` だけでよい。配置は運用者が行う。

- `--url` にはその host の t12 node の JSON wRPC を渡す。localhost の listen port で、`misakascan.com/kaspa` が proxy
  している先。
- 公開 endpoint を渡してもよい。

```cron
17 * * * * mkdir -p /root/.t12-panel-bias && python3 /root/t12-monitor/panel_bias.py --url ws://127.0.0.1:<json-wrpc-port> > /root/.t12-panel-bias/last-report.txt 2>&1; tail -1 /root/.t12-panel-bias/last-report.txt >> /root/.t12-panel-bias/summary.log
```

- 負荷は 1 時間に数十の read call だけ。
- 公開 node の host に置くので、demo 用の node を立てたり、他の unit に触れたりはしない。

## 限界（読み方）

- **anchor producer が 1 つしか無い時間帯は比べる相手がいない。** その層の claim は relative 検定に情報を与えない
  （`comp/claims` の分子に入らない）。その間の偏りは model note にしか出ず、母集団全体の効果と区別できない。
  現状は g1（ibm）と g6（.113）が anchor しているので比較できる。
- **producer が 2 つのとき**、relative 検定は「2 つが違う」までしか言えない。どちらかは model 側の数字で判断する
  （model 自体が母集団効果でずれていると、この判断も揺れる）。
- **bind の瞬間の Valid-lock filter**（free stake で bind の lock を張れない seat の除外）は、後から RPC で見えない。
  model note には効くが、relative 検定には効かない（全 producer に等しく効く）。
- **ADR-0147 の outsider seat はモデル化していない。** 対象は bought class だけで、t12 の genesis class には無い。
- **maturity の settled-anchor floor による窓の拡張もモデル化していない**（model note だけに効く）。
- **lane A の seed source**（slot 以降で最も早い operator attempt、anchor block とは限らない）は読んでいない。上記のとおり、
  lane A 以降の帰属は anchor block の producer で行う。
