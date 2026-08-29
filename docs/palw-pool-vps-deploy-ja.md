# PALW マイナープールを VPS で立てて testnet-11 で使う

**対象読者は 2 人います。** プールを VPS に立てる**運用者**（§1–§4）と、そのプールに
ノード無しでつなぐ**マイナー**（§5–§7）。どちらも testnet-11 の実手順です。

前提の設計は [palw-mining-pool-runbook-ja.md](palw-mining-pool-runbook-ja.md) にあります。
一行で言えば: **プールはノードが要る仕事を、マイナーは鍵が要る仕事を持つ。プールは鍵も
報酬も持たない**（coinbase はマイナー自身のアドレスに直接支払われます）。

---

## 1. VPS の要件

| | プール（運用者） | マイナー |
|---|---|---|
| CPU | 2 vCPU 以上 | 1 vCPU から（推論と nonce 探索がそのまま仕事量） |
| RAM | **4 GB 以上**（チェーン状態 + mempool） | 512 MB 程度（floor クラスは軽量） |
| ディスク | **20 GB 以上**（チェーン + material 保持） | 100 MB（バイナリと鍵だけ） |
| 開放ポート | **26311/tcp**（p2p）と **26350/tcp**（プール） | **なし**（発信のみ） |
| 秘密情報 | **無し** | ML-DSA-87 seed 1 個 |

マイナー側に開放ポートが要らない点が要です — NAT 配下でも自宅回線でも動きます。

## 2. ビルド

```bash
# 依存（Ubuntu/Debian）
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev protobuf-compiler git curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

git clone https://github.com/MISAKA-BTC/misakas.git
cd misakas

# プール側（ノード）
cargo build --release -p kaspad --bin kaspad

# マイナー側（ノード不要）
cargo build --release -p misaka-palw-pool --bin misaka-palw-pool-miner
```

`protobuf-compiler` は必須です（p2p の protobuf 生成に `protoc` が要る）。無いと
`Could not find protoc` でビルドが落ちます。

## 3. プールを起動する（運用者）

```bash
./scripts/misaka-palw-pool-vps.sh up
```

このスクリプトは起動後に **2 つの行を待って確認します**:

```
Consensus params fingerprint: 15bab795442ec3ef… (network testnet-11)
[palw-pool] listening on 0.0.0.0:26350 for class <128-hex> (up to 256 miners; each needs its own registered bond)
```

**fingerprint が違えば別ネットワークです。** その状態のプールにつないだマイナーは、
自分が参加しているつもりのないチェーン向けに働くことになるので、スクリプトは警告を出します。

手で起動する場合:

```bash
kaspad --testnet --netsuffix=11 --utxoindex \
  --appdir=$HOME/.misaka-t11 \
  --listen=0.0.0.0:26311 \
  --rpclisten=127.0.0.1:26312 \
  --palw-pool-listen=0.0.0.0:26350 \
  --palw-pool-max-miners=256
```

**プールに `--palw-producer-key` / `--palw-producer-bond` / `--palw-producer-pay-address` は
不要です。** それがプロデューサを走らせることとの違いです。自分でも掘りたい場合のみ
`--palw-produce` と上記 3 つを足します（同居可能・retention ディレクトリは共有）。

### systemd で常駐させる

```bash
sudo useradd -r -m -d /var/lib/misaka -s /usr/sbin/nologin misaka
sudo install -m755 target/release/kaspad /usr/local/bin/kaspad
sudo install -m644 scripts/misaka-palw-pool.service /etc/systemd/system/
sudo systemctl daemon-reload && sudo systemctl enable --now misaka-palw-pool
journalctl -u misaka-palw-pool -f
```

### ファイアウォール

```bash
sudo ufw allow 26311/tcp comment 'palw p2p'
sudo ufw allow 26350/tcp comment 'palw pool'
sudo ufw enable
```

**RPC(26312) は開けないでください。** ループバック固定にしてあります —
`--unsaferpc` を公開面に出すのは、チェーンを持つノードへの無認証の扉です。
プールポートはボンドのハンドシェイクで認証されるので、開けてよい唯一のポートです。

### 状態を見る

```bash
./scripts/misaka-palw-pool-vps.sh status
```

```
node:        running (pid 12345)
fingerprint: Consensus params fingerprint: 15bab795442ec3ef… (network testnet-11)
pool:        [palw-pool] listening on 0.0.0.0:26350 for class …
port:        26350 is listening
--- recent pool activity ---
[palw-pool] 203.0.113.9:51234 says hello for bond a1b2…:0 (miner/1)
[palw-pool] 203.0.113.9:51234 admitted on a bond the chain holds
[palw-pool] 203.0.113.9:51234 produced block 7f3c…
```

## 4. 同期を待つ

プールは**同期が終わるまでジョブを配りません**（配れば、そのブロックはチェーンに拒否されます）。
その間マイナーには理由付きの `standby` が返り、マイナーは待って再要求します:

```
[miner] the pool has no work right now (this node is still syncing); asking again in 2000 ms
```

これは正常です。testnet-11 の同期は数分程度です。

---

## 5. マイナー: 鍵とボンドを用意する（一度だけ）

**ボンド登録にだけノードが要ります。** 一度きりで、自分のノードでも、上で立てたプール用
ノードでも、信頼できる誰かのノードでも構いません。

```bash
# 鍵
cargo build --release -p misaka-cli --bin misaka
./target/release/misaka key gen --out miner-seed.hex
chmod 600 miner-seed.hex          # 群/他ユーザ可読だと読み込みが fail-closed で拒否されます
./target/release/misaka key address --key-file miner-seed.hex    # → misakatest:...
```

そのアドレスに **通常送金で** MSK を入れます（coinbase 出力は成熟規則とスキャン対象外で
使えません — 詳細は [testnet11-join-mining.md](testnet11-join-mining.md) §2）。

```bash
# ボンド登録（この 1 回だけノードが要る）
kaspad --testnet --netsuffix=11 --appdir=$HOME/.t11-bond \
  --palw-register-bond \
  --palw-producer-key=$PWD/miner-seed.hex \
  --palw-producer-pay-address=<上のアドレス>
```

成功すると **1 行だけ** outpoint が出ます。**この行が唯一の出所です**（outpoint はその
トランザクション自身の id で、事前には存在しません）:

```
[palw-panel] registered bond <txid>:0 with <n> sompi of collateral, in tx <txid>.
```

担保額は `--palw-bond-collateral` で指定できます。既定はチェーン最小ではなく
「1 claim が収まる額」を自動算出します。**relay 制限のほうが先に効き**、testnet-11 では
実質 **8,333,924 sompi** 程度が下限です（KIP-0009 のストレージ質量は出力が小さいほど増える）。
詳細と回避は join-mining §3 に。

登録後、**このノードは落として構いません**。ボンドはチェーン状態で、どのプールからも参照できます。

## 6. マイナーを起動する（ノード不要）

```bash
./target/release/misaka-palw-pool-miner \
  --pool <プールのホスト>:26350 \
  --bond <txid>:<index> \
  --key miner-seed.hex \
  --pay-address misakatest:...
```

正常な出力:

```
[miner] admitted on misaka-testnet-11; class <128-hex> (the derived floor — nothing to download)
[miner] job j12: no winner in the assigned range (3.4s)      ← 通常。次へ
[miner] job j13: WON at nonce 918273 after 918274 tries
[miner] block <hash> accepted — the coinbase pays this miner's address
```

**モデルのダウンロードはありません。** floor クラスの重みは pinned seed から起動時に導出され、
チェーンが登録した `artifact_root` と一致しなければ拒否されます。

### systemd で常駐させる

```bash
sudo useradd -r -m -d /var/lib/misaka-miner -s /usr/sbin/nologin misaka-miner
sudo install -m755 target/release/misaka-palw-pool-miner /usr/local/bin/
sudo install -m600 -o misaka-miner miner-seed.hex /var/lib/misaka-miner/seed.hex
sudo install -m644 scripts/misaka-palw-pool-miner.service /etc/systemd/system/
sudo systemctl edit misaka-palw-pool-miner     # POOL / BOND / PAY_ADDRESS を設定
sudo systemctl daemon-reload && sudo systemctl enable --now misaka-palw-pool-miner
journalctl -u misaka-palw-pool-miner -f
```

## 7. 支払いを確認する

coinbase はマイナー自身のアドレスに直接支払われるので、**プールを信用せずに**
エクスプローラで確認できます: [misakascan.com](https://misakascan.com) で
`--pay-address` に指定したアドレスを検索してください。

マイナー側でも毎ジョブ検証しています — テンプレートの全 tx から `hash_merkle_root` を
再計算してヘッダと一致を確認し、coinbase の出力に自分の script があるかを見ています。
**プールが自分に払っていないテンプレートは、推論を 1 回も使う前に拒否されます**:

```
[miner] job j14 refused: the coinbase of this template pays another script — this pool is not
        building blocks that pay this miner
```

---

## 7b. プールが負う data-availability 義務（設計上の要点）

PALW の claim は、シートがその実行の material を**取り寄せて**検証できて初めて license されます
（protocol 104 以降の pull 方式・監査 M2-22）。material を持たない主体は court に応答できません。

**この設計ではプールが持ちます。** マイナーは自分に P2P の口が無いので、solution と一緒に
material をプールへアップロードし、プールは **ブロック投函より先に** それを retention
ディレクトリへ保存してから gossip します。順序は義務が先・約束が後です。

プールノードは起動時に 2 つを行います:

* **material pull への応答登録** — `set_material_resolver` を retention ディレクトリに対して登録。
  これが無いとプールは「バイトはディスクにあるのに、訊かれても黙っている」状態になり、
  claim は quorum を得られず receipt 期限で void し、**マイナーは本当にやった仕事の対価を
  受け取れません**。panel も同じディレクトリに同じクロージャを登録するので、両方動かしても
  影響はありません。
* **retention の剪定**（監査 M2-22）— lattice の地平（bind + receipt ≒ 48h）を過ぎた material は
  削除します。これが無いと consensus ボリューム（RocksDB と同じボリューム）上で
  単調増加します。

どちらも `misaka-palw-pool` 側ではなく **kaspad のプールサービス**の仕事です
（P2P の口を持つのはノードだからです）。ログで確認できます:

```
[palw-pool] answering material pulls from /var/lib/misaka/t11/palw-retention
[palw-pool] pruned 3 retained material file(s) past the lattice horizon
```

**ボンドをプールに持たせない理由もここにあります。** プールがボンドを持ち採掘者が別マシンで
推論する形にすると、プールは自分が計算していない実行について court に応答することになり、
material の授受を誤ると自分のボンドがスラッシュされます。ボンドをマイナー側に置くと、
claim もスラッシュ責任も実際に計算した主体に残り、プールが負うのは
「預かったバイトを配る」——失敗しても void で済み、conviction にはならない義務だけになります。

## 8. 困ったときに読む順番

| 症状 | 見るところ |
|---|---|
| `this chain holds no bond at <outpoint>` | §5 を実行。**ボンド未登録が最頻の原因** |
| `the auth signature did not verify…` | ボンドは合っているが `--key` が違う seed |
| `the chain refuses this bond for now: …` | epoch budget 切れ／exposure 上限。チェーンの言い分どおり待つか担保を増やす |
| `another session already holds bond …` | 同一ボンドで 2 台接続している。1 ボンド 1 台 |
| `standby (this node is still syncing)` | 正常。プールの同期待ち（§4） |
| プールに繋がらない | ファイアウォール（§3）と `ss -ltn | grep 26350` |
| fingerprint が違う | ビルドが古い。`git pull && cargo build --release` |
| ブロックは通るのにマイナーが支払われない | プールの `answering material pulls from …` 行があるか確認（§7b）。無ければ material が配られていません |
| `Genesis mismatch … local: d25a80b9…` | 退役済みチェーンの appdir。appdir を消して再同期 |

## 9. この文書が保証していないこと

* **testnet-11 実測値はまだありません。** 本番ネットワーク上での 1 ジョブあたり所要時間、
  受理率、1 日あたりの期待ブロック数は、走らせた人が測る数字です。
  実装は実ソケット・実推論・実署名で検証済みですが（`misaka-palw-pool` の 27 テスト、
  うち end-to-end 3 本）、その 3 本のチェーンはフェイクで、class target は
  探索が終わる値に開けてあります。
* **分散共有はしません。** アンカーがボンドに束縛されるため共有探索空間が存在せず、
  取り除いたのはノード要件であって分散ではありません（設計の詳細は
  [palw-mining-pool-runbook-ja.md](palw-mining-pool-runbook-ja.md) §0）。
* **material 不配のリスクはプール側に残ります。** material を受け取って gossip しない
  プールは license されない claim を作り、報酬はゼロになります。これは流動性リスクであって
  スラッシュリスクではありません（正直な execution は誰が配り損ねても convict されません）。
