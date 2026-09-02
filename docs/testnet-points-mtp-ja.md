# MISAKA テストネットポイント (MTP) — 参加方法とポイントの確認方法

> **テストネット専用で、価値はありません。** MTP ポイントはテストネットへの参加記録です。トークン
> でも残高でもなく、譲渡もできず、**金銭的価値はありません**。`testnet-10` の MSK も同様に無価値な
> テスト用通貨です。メインネットも販売も存在せず、ポイントが何かに換わるという約束もありません。
> MISAKA のポイントやテストネット MSK の売買を持ちかけてくる相手は詐欺です。

ポイントは **ML-DSA-87 で署名された epoch 台帳**の写しです。以下はすべてオフラインで検証できます —
数値を配信するサーバを信用する必要はありません。採点対象ネットワークは **`testnet-10`** です。

（英語版: [testnet-points-mtp.md](testnet-points-mtp.md)）

- [1. 今ポイントになるもの](#1-今ポイントになるもの)
- [2. 参加手順](#2-参加手順)
- [3. ポイントの確認方法](#3-ポイントの確認方法)
- [4. 台帳を自分で検証する](#4-台帳を自分で検証する)
- [5. epoch — いつから数え始まるか](#5-epoch--いつから数え始まるか)
- [6. 現在のステータス](#6-現在のステータス)
- [7. FAQ](#7-faq)

---

## 1. 今ポイントになるもの

`testnet-10` では 2 つのカテゴリが**チェーンから自動で**加算されます。登録もアカウントも申請も不要
です。運営側の毎時ジョブが canonical チェーンを走査し、正しい形式の `misakatest:` アドレスに対して
その実績を加算します。

| | カテゴリ | 加算条件 | レート |
|---|---|---|---|
| C1 | node | その epoch window 内で**受理されたブロックを 1 つ以上採掘**（ブロックの払い出しアドレスに加算） | **1 epoch あたり 200 ポイント固定**。採掘量を増やしても増えません |
| C3 | verify | その window 内の**受理されたトランザクション** | **100 件ごとに 1 ポイント**（切り捨て。epoch 単位の上限あり） |
| C2 | bug | 重大度別のバグ報告（S0 5000 / S1 2000 / S2 500 / S3 100）。重複報告は初報の 10 % | レビュー後に運営が付与 |
| C4 | infra | インフラ貢献 | レビュー後に運営が付与 |
| C5 | LLM replica | 受理され k=2 一致した PALW replica 作業。1 スロット 1 ポイント固定 | 収集機構は実装済みだが、**現在の `testnet-10` 毎時ジョブには含まれていません** |

C1 の固定レートは丸めた説明ではなく、実際の台帳がそう払っているという事実です。現在の fact store
では **391,635 ブロック**採掘したアドレスと **764 ブロック**のアドレスが、同じ window で**どちらも
ちょうど 200 ポイント**です。C1 は「数えるに値するノードを動かしたこと」への対価であり、ハッシュ
レートの量を測るものではありません。したがって大規模マイナーと小規模マイナーは C1 では横並びです。

C3 は同じ考え方を利用実績に当てはめたものです。受理 tx 4,101 件 → 41 ポイント、32 件 → 0 ポイント。
tx は **canonical チェーンで受理**されている必要があります。後で reorg で消えるブランチ上の活動は
仕様として数えません（`docs/testing/mtp-epoch2-partition-policy.md`）。

**C2 と C4 は運営がレビュー後に手動付与**します。運営が帰属を直接宣言するため、登録は不要です。

## 2. 参加手順

### 手順 1 — アドレスを作る

この鍵が**ポイント上のあなたの識別子そのもの**です。復旧手段はないのでバックアップしてください。

```bash
cargo build --release --bin misaka
./target/release/misaka key gen --network testnet-10 --out mtp.seed
```

`misakatest:…` アドレスが表示されます。これがそのまま台帳 id で、リーダーボードには
`addr:misakatest:…` として並びます。**C1/C3 を稼ぐのに必要な手続きはこれだけです。** 2026-08-02 以降、
正しい形式のアドレスはオンチェーン実績に対して登録なしで採点されます。従来の招待状／登録ハンドシェイ
クは、ポイントを GitHub ハンドル (`gh:<you>`) に紐付けたい場合にのみ必要です。

### 手順 2 — testnet-10 のノードを動かす

```bash
cargo build --release --bin kaspad
./target/release/kaspad --testnet --netsuffix=10 --utxoindex \
  --addpeer=160.16.131.119:26211
```

`misaka join --network testnet-10` は DNS seed を明示してくれる初心者向けの入口です。何かが加算され
ることを期待する前に、**実際に参加できていて同期済みか**を確認してください。IBD 中のノードは接続で
きても使用可能ではなく、何も加算されません。

```bash
./target/release/misaka node doctor --network testnet-10
```

### 手順 3 — 自分のアドレス宛に採掘する（C1）／チェーンを使う（C3）

```bash
cargo build --release --bin kaspa-pq-miner
./target/release/kaspa-pq-miner --network-id testnet-10 --rpc 127.0.0.1:26610 \
  --pay-address misakatest:<your-address> --blocks 0 --min-block-interval-ms 1000
```

window 内に受理ブロックが 1 つあれば C1 の 200 ポイント満額です。そのアドレスからの送金
（`misaka wallet send …`）は受理 tx 100 件ごとに 1 ポイントの C3 になります。

### 任意 — GitHub ハンドルでの識別

ポイントを `gh:<handle>` に貯めたい場合（および C2/C4 をそのハンドルで受け取りたい場合）は、この
リポジトリに issue を立ててハンドルとアドレスを伝え、返ってきた招待状をオフラインで署名して提出しま
す。

```bash
./target/release/misaka mtp register --network testnet-10 \
  --invitation invitation.json --key-file mtp.seed --out registration.json
```

MTP の HTTP 面は**設計上リードオンリー**です。登録用エンドポイントは存在せず、したがって偽造された
登録を受け付けうる口も存在しません。1 ハンドルにつき 1 アドレスで、登録前の実績は遡及しません。

## 3. ポイントの確認方法

公開・リードオンリーの照会 API です。アカウントもログインも不要です。

```
https://misakascan.com/mtp/v1/...
```

| ルート | 返すもの |
|---|---|
| `/mtp/v1/points` | 全 id のリーダーボード（順位付き） |
| `/mtp/v1/points/<id>` | 単一 id（例 `addr:misakatest:qtp…` / `gh:alice`） |
| `/mtp/v1/epoch/<n>` | epoch `n` の署名済み台帳（最新 issue） |
| `/mtp/v1/epoch/<n>/facts` | その台帳が採点した入力そのもの |
| `/mtp/v1/epoch/<n>/all` | epoch `n` の全 issue（差し替えられた過去分を含む） |
| `/mtp/v1/operator` | 運営の 2592 バイト ML-DSA-87 公開鍵と、その帯域外 pin |
| `/mtp/v1/rules/<hash>` | その台帳が採点された凍結ルールセット |

```bash
curl -s https://misakascan.com/mtp/v1/points                        # リーダーボード
curl -s https://misakascan.com/mtp/v1/points/addr:misakatest:<your-address>
```

運営鍵は帯域外でここに pin してあります。エンドポイントが「誰が署名しているか」について嘘をついて
いないか確認できます。

```
misakatest:qtu8yq0psff2leaz35rqrh5kcz20kug5jce2ecca9hx7ed6cxpghrzhnjg650ugu7esa8snj2ltz4v0dkdzu0dn7s90xmakw0fneety0pvngw4r0
```

`/mtp/v1/operator` の `pins` に同じ文字列が現れなければ、そのサーバが配る台帳を信用しないでください。

**同梱 CLI は HTTPS エンドポイントに到達できません。** `misaka mtp points` / `misaka mtp leaderboard`
は TLS なしの素の HTTP/1.1 なので、`http://` のインスタンス（ローカルのサービスか、そこへのトンネル）
に対してのみ動作します。

```bash
misaka mtp points addr:misakatest:<your-address> --endpoint http://127.0.0.1:8790
misaka mtp leaderboard --endpoint http://127.0.0.1:8790
```

これはクライアント側の制限であって、信頼モデルの穴ではありません。HTTPS は `curl` で取得し、**検証は
ローカルで**行ってください。実際に何かを証明するのはそちらです。

## 4. 台帳を自分で検証する

```bash
curl -s https://misakascan.com/mtp/v1/operator \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["operator_pubkey_mldsa87_hex"])' > operator.pub

curl -s https://misakascan.com/mtp/v1/epoch/1        > epoch-1.jsonl
curl -s https://misakascan.com/mtp/v1/epoch/1/facts  > epoch-1.input.json

misaka mtp verify-epoch epoch-1.jsonl --pubkey-file operator.pub
misaka mtp verify-epoch epoch-1.jsonl --pubkey-file operator.pub --facts epoch-1.input.json
```

`--facts` なしなら ML-DSA-87 署名と rules hash を検査します。`--facts` を付けると**採点を決定的に再実
行**し、署名済み台帳とバイト単位で比較します。採点は全て整数演算（ポイントは milli-point 単位で保持
されるため、台帳の `"c1": 200000` は 200 ポイント）なので、どのプラットフォームでもビット単位で再現
します。両方を通った台帳は、サーバを信用した結果ではありません。

すべての fact は証拠を伴います。C1 の行はブロックハッシュを、C3 の行は txid を持ちます。自分の
ポイントがおかしいと思ったら、`/mtp/v1/epoch/<n>/facts` に「何が数えられたか」がそのまま出ています。

## 5. epoch — いつから数え始まるか

epoch は運営が発行する週次 window `[月曜 00:00:00Z, +7 日)` です。明示しておくべき帰結が 2 つあります。

- **最初の epoch 発行より前には何も積み上がらず、遡及補填もありません。** fact は継続的に収集されま
  すが、それを含む epoch が発行されて初めてポイントになります。
- **発行は明示的な運営コマンドであり、cron ではありません。** 署名済み台帳は参加者が依拠してよい成果
  物なので、タイマーが無人で吐くものにはしていません。

訂正は例外ではなく設計された経路です。再発行は完全に署名し直した新しい
`epoch-<n>.<issue>.jsonl` で、古い issue が削除されることはなく、差し替え順序は `index.json` に記録
されます。epoch が不変になるのは finality horizon が通過した後だけです。

**これまでの発行状況**

| epoch | window (UTC) | 状態 |
|---|---|---|
| 1 | 2026-08-07 00:00 → 14:29 | 発行済み（issue 0、`finalized: false`） |
| — | 2026-08-08 → 2026-08-15 | **スキップ＝不採点**。その週はネットワークが分断・停止しフラグデーを実施したため。運営側の障害で参加者を採点するのは誤り（`docs/testing/mtp-epoch2-partition-policy.md`） |
| 2 | 2026-08-15 00:00 → 2026-08-22 00:00 | 予定 |

採点 window が 2026-08 のフォークに掛かる場合、**canonical な系統だけ**が数えられます。運営・premine・
運営機のアドレスも他と同じルールで採点され、隠すのではなく「運営運用」と明示されます。現在リーダー
ボード上位にいるのは運営のコールドスタート・マイナーです。

## 6. 現在のステータス

**2026-08-15 時点で、収集は停止中・照会 API はメンテナンスのため停止中です。** これが見えること自体
が公開する意味です。

- 最後に取り込まれたチェーン fact は **2026-08-12 01:28Z** のものです。08-12 06:00Z 以降は毎時走査が
  新規ブロックを見つけられず（チェーン停止のため）、08-14 17:00Z 以降は explorer データベースが停止し
  ているため走査自体が失敗しています。
- `misaka-mtp.service` は 2026-08-15 01:54 JST に explorer スタックごと停止されたため、
  `https://misakascan.com/mtp/v1/...` は現在 **502** を返します。
- 発行済み台帳は epoch 1 のみのままです。ポイントが失われることはありません。fact はチェーンから再取
  り込み可能で、収集側は `(kind, evidence, address)` で重複排除するため、復旧後に広い範囲を再走査すれ
  ば二重計上ではなく欠損の穴埋めになります。

## 7. FAQ

**登録は必要ですか？** C1/C3 には不要です。正しい形式の `misakatest:` アドレスなら、自分のオンチェーン
実績に対して加算されます。登録は `gh:<handle>` の識別子が欲しい場合だけです。

**たくさん採掘したのに 200 ポイントしかありません。** C1 は仕様として epoch 単位の固定値です。採掘量
を増やしても増えません。追加のハッシュレートは測定対象の貢献ではないからです。

**トランザクションが加算されません。** C3 は受理 tx **100 件**につき 1 ポイント（切り捨て）です。99 件
なら 0 ポイントです。また tx は canonical チェーンで受理されている必要があります。

**自分の活動が 2026-08-08 〜 08-15 の週に入っています。** その週は運営自身も含め全員が不採点です。誰か
を狙った罰則ではなく、その週はネットワークが分断されていたためです。

**採掘直後にポイントが出ません。** ポイントはリアルタイムではなく、それを含む epoch が発行された時点で
反映されます。fact の収集は毎時、台帳の発行は epoch 単位です。

**サイトの数値は正か？** 正なのは*台帳*で、サイトはその写しです。`misaka mtp verify-epoch --facts` で
検証し、ページではなくその結果を信じてください。

---

**関連** — [`docs/testing/mtp-epoch2-partition-policy.md`](testing/mtp-epoch2-partition-policy.md)
（2026-08 の分断に関する運営決定）、[`docs/validator-runbook.md`](validator-runbook.md)
（validator の運用）。
