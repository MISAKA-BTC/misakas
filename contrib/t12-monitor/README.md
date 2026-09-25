# testnet-12 panel-bias monitor（panel-seed stopgap の lane C）

作成: 2026-09-26（branch `rcore/panel-bias-monitor`、`rcore/int-3` の tip 基準）。**どこにも install していない。**
公開 t12（`wss://misakascan.com/kaspa`）に対して手で 2 回走らせ、動くことだけを確かめた。

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

**anchor block の決め方。** anchor は、`bind_base + anchor_delay` 以上の DAA を持つ、selected chain 上で最初の attempt
block（algo 6/9）である。`bind_base` は `reboundDaa`、無ければ `acceptedDaa`。根拠は `processor.rs` の
`palw_v2_anchor_fact_of_candidate` と `palw_block_may_anchor_a_panel_v1`。

- SW-8 により panel はその anchor block で bind する。したがって anchor の DAA は `boundDaa` に一致するはずで、monitor は
  bound claim ごとにこれを確かめる。一致しなければ DEGRADED にする。
- 2026-09-26 01:08 の実測では、bound 64 件がすべて一致した。
- anchor block の producer は、その header の PAV2 envelope に入っている executor bond から読む。

**統計。** 帰無仮説 H0 は「各 panel は、モデル化した母集団からの stake-weighted successive sample である」。

- **draw のモデル。** exponential race（ADR-0152 SW-2/SW-3）で、重みは posted collateral（MSK 単位、1,000,000 MSK で cap）。
  t12 では operator = bond。
- **母集団。** claim の bind を最初に観測した run で作る。
  - executor は除く。
  - base class（floor）は、その class を宣言している bond。
  - それ以外の class は、`getPalwPanelSeats` の row が `ready` の bond。
  - Active で、seat floor（130,000 MSK）以上であること。
  - 非 genesis bond は maturity を満たす場合だけ入れる（DAA 1,000 以降は、登録から 1,000 DAA）。
  - 実際に座った bond は、その claim の母集団に必ず入れる。
- **cell 検定。** (anchor producer, seat bond) ごとに、その producer が anchor した claim で bond が座った回数を数える。H0 の下で
  この回数は正確に Poisson-binomial に従う。claim ごとの Bernoulli で、確率はその draw での inclusion probability
  （厳密な DP で計算する）。
  - p 値は正確な両側 p 値。
  - Holm–Bonferroni で、run 内の全 cell を family-wise `--alpha` に抑える。
- **omnibus 検定。** anchor producer ごとに Pearson X² を計算する。
  - 非復元抽出で母集団が claim ごとに違うので、Rao–Scott の second-order（Satterthwaite）補正をかける。
  - anchor producer 間は Bonferroni で `--alpha` に抑える。
  - 期待値がすべて `--min-expected`（5）以上のときだけ評価する。
- **false alarm 率。** 既定の `--alpha 1e-4` で、1 run あたり ≤ 2×10⁻⁴。hourly に回すと union bound で ≤ 0.5%/日。
  連続する run はデータを共有するので、実際はこれより低い。
  - synthetic の較正（unit test）: Rao–Scott の棄却率は名目 0.05 に対して 0.037。
  - 補正なしの Pearson は 0.00 だった（5-of-7 の非復元抽出は Poisson より揺れが小さいため）。
  - 2 family・α=0.05 の false alarm は 0.02。
- **検出力（synthetic）。** 1 つの anchor producer が、自分の panel に特定の 2 seat を毎回座らせる grind をした場合、α=1e-4 で
  次の検出率になる。

  | その producer が anchor した claim 数 | 検出率 |
  |---|---|
  | 約 30 | 0.80 |
  | 約 40 | 0.97 |
  | 約 60 | 1.00 |

  t12 は約 2.8 claim/DAA なので、数時間分のデータにあたる。

**alert の種類**（exit 1）:

- **bias** — cell または omnibus が有意なとき。両側なので、特定 seat を**外す**側の偏りも出る。
  - pooled 行（全 anchor 合算）が同じ向きに有意な場合は「population note」として別に出す。これは母集団全体の効果
    （saturation・readiness 切れ等）であり、それだけで anchor 固有の偏りとは言えない。
- **external seat** — panel に非 genesis bond が座ったとき。初出の claim だけを alert し、`--realert` で再掲する。
- **external anchor** — 非 genesis bond の attempt が anchor になったとき（bind 無しで void された claim の anchor も含む）。
  stopgap (A) の fence 以後は 0 件でなければならない。

**DEGRADED**（exit 2）: RPC が失敗したとき、応答が truncated のとき、anchor が boundDaa と一致しないとき、
consensus params fp が出荷値（`b8564b88…`）と違うときに立つ。fp が違うのは post-launch fence を武装した後で、その時は
母集団規則を見直してから `--expect-fp <新 fp>` を渡す。alert は DEGRADED より優先する。

## 出力

人間向けの report を stdout に出し、最後の 1 行に machine 用の summary を出す。

```
PANEL_BIAS {"status":"OK","time":...,"tip":47,"fp":"b8564b888e55bb5f","claims":129,"bound":70,"tested":70,
            "anchors":{"g1":54,"g6":16},"ext_seat":0,"ext_anchor":0,"bias_cells":0,"min_cell_p":0.0646,
            "min_omnibus_p":0.167,"alpha":0.0001,"alerts":[],"degraded":[]}
```

- 実際の出力は 1 行。
- `--quiet` を付けるとこの行だけを出す。
- `--json FILE` を付けると、全 record と全検定結果を書き出す。
- exit code: 0 = OK、1 = ALERT、2 = DEGRADED / 評価できない。

report の中身:

- 最近の panel の一覧。seat の後ろの `V` は valid、`.` は pending、`-` は none、`*` は full seat。
- anchor producer × seat bond の observed/expected 表。有意な cell には `!` が付く。
- host 別（ibm / 5.104 / .113 / external）の observed/expected。
- 検定の結果と alert。

## 走らせ方

```sh
# 1 回（Mac から公開 endpoint へ。~25 read call、30〜60 s）
python3 contrib/t12-monitor/panel_bias.py
# state は ~/.t12-panel-bias/state.json（--state で変える、'' で保存しない）

# 保存済み state だけで再解析（network 無し）
python3 contrib/t12-monitor/panel_bias.py --offline

# test（network 無し、~3 s）
python3 -m unittest -v contrib/t12-monitor/test_panel_bias.py
```

主な option:

| option | 意味 |
|---|---|
| `--alpha` | 検定 family ごとの family-wise 誤警報率（既定 1e-4） |
| `--window-daa N` | 直近 N DAA に anchor した claim だけを検定する（既定は全記録） |
| `--retain-daa` | retire した claim を何 DAA 分保持するか（既定 20,000 ≈ 4 週間。state は約 0.6 KB/claim） |
| `--realert` | external seat / anchor の alert を毎回出す |
| `--roster FILE` | genesis bond の名前（`{"0": {"host": "ibm", "name": "..."}}`）を差し替える |
| `--url` | endpoint（`ws://` / `wss://`） |

### hourly（Mac の cron）— install はしていない、例だけ

`crontab -e` に次を足す（1 行、paths は checkout に合わせる）:

```cron
7 * * * * mkdir -p $HOME/.t12-panel-bias && /usr/bin/python3 /Users/wata/Downloads/MISAKA-wt-b/wt-monitor/contrib/t12-monitor/panel_bias.py > $HOME/.t12-panel-bias/last-report.txt 2>&1; rc=$?; tail -1 $HOME/.t12-panel-bias/last-report.txt >> $HOME/.t12-panel-bias/summary.log; [ $rc -eq 0 ] || osascript -e "display notification \"exit $rc - see ~/.t12-panel-bias/last-report.txt\" with title \"t12 panel-bias\""
```

- `summary.log` には 1 時間に 1 行の JSON がたまる。
- 0 以外の exit で通知が出る。1 は alert、2 は degraded。
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

- **母集団は claim の bind 後に最初に観測した run の値。**
  - 8k の readiness は観測時点の `ready` を使う。
  - bind の瞬間の Valid-lock filter（free stake で bind の lock を張れない seat の除外）は、後から RPC で見えない。
  - このため、saturation した seat も母集団に入れている。これは全 anchor producer に等しく効き、pooled 行の
    「population note」に出る。
- **ADR-0147 の outsider seat はモデル化していない。** 対象は bought class だけで、t12 の genesis class には無い。
- **maturity の settled-anchor floor による窓の拡張もモデル化していない。**
- **F1（seed = execution commitment）が武装されても、この monitor はそのまま使える。** anchor block とその producer という
  概念は変わらないからである。stopgap (A) の武装後は、external anchor alert が出れば consensus 側の失敗を意味する。
- **registry-resilience lane の再 anchor（次の slot での redraw）も読める。** `reboundDaa` を `bind_base` として読むので、
  そのまま扱える。
