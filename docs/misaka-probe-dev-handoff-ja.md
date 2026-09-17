# misaka-probe PoC 引き継ぎメモ

このメモは、IPを指定して「node参加できているか」「DNS seederとして見えているか」「validator登録状態を確認できるか」を試すためのPoCです。

現時点では、mainのRust CLIへ入れる前の確認用として、Bashスクリプトを用意しています。

```text
scripts/misaka-probe.sh
```

## 目的

運用者が以下を1コマンドで確認できるようにします。

- 対象IPの `26211/tcp` が外部から到達できるか。
- seed domainのA応答に対象IPが含まれているか。
- 対象IPがDNS seederとして `53/udp` / `53/tcp` で複数Aレコードを返すか。
- 自分のVPS上で `misaka-kaspad` / `misaka-dnsseeder` / `misaka-validator` が動いているか。
- 任意で `stake-bond` を渡した場合、そのbondがactiveか。

## 重要な制限

IPだけでvalidator参加は確定できません。

理由は、validatorはIPではなく以下で識別されるためです。

- `validator_id`
- `stake bond outpoint`

つまり、IPだけで分かるのは主にnode/DNS seederの到達性です。

validatorについては、`--stake-bond <txid>:0` を渡した場合のみ、on-chain registry上の状態を確認できます。

## VPSでのインストール

VPS上の `/opt/misakas` にこのrepoがある前提です。

```bash
cd /opt/misakas
install -o root -g root -m 0755 scripts/misaka-probe.sh /usr/local/bin/misaka-probe
```

確認します。

```bash
misaka-probe --help
```

## 使い方

自分のVPS IPを確認する場合です。

```bash
misaka-probe --ip 217.76.57.217
```

validatorのbond作成後は、bond outpointも渡せます。

```bash
misaka-probe --ip 217.76.57.217 --stake-bond <txid>:0
```

RPCやseed domainを明示する場合です。

```bash
misaka-probe \
  --ip 217.76.57.217 \
  --network testnet-10 \
  --rpc 127.0.0.1:27210 \
  --seed seeder2.misakascan.com
```

## 出力の見方

主な判定は3つです。

```text
Node verdict
DNS seeder verdict
Validator verdict
```

### Node verdict

```text
NODE_OK
```

P2P portに到達でき、seed domainにも対象IPが含まれている状態です。

```text
NODE_REACHABLE
```

P2P portには到達できますが、seed domainに対象IPが見えていません。

```text
NOT_REACHABLE
```

P2P portに到達できません。

### DNS seeder verdict

```text
DNS_SEEDER_OK
```

対象IPの `53/udp` と `53/tcp` が、seed domainに対して複数Aレコードを返しています。

```text
PARTIAL_DNS_SEEDER
```

UDPかTCPの片方だけ成功しています。

```text
NOT_A_DNS_SEEDER_OR_NOT_PUBLIC
```

DNS seederとしては応答していません。通常nodeならこの状態でも問題ありません。

### Validator verdict

```text
UNKNOWN
```

`--stake-bond` がないため、IPだけではvalidator参加を判定できません。

```text
VALIDATOR_REGISTERED_ACTIVE
```

渡した `--stake-bond` がregistry上でactiveです。

ただし、これでも「そのIPがそのvalidator keyを実行している」ことまでは証明しません。IPとvalidatorの紐付けはプロトコル上の直接IDではないためです。

## devへ渡す時の説明

このPoCは、まず運用確認のUXを試すためのものです。

mainに取り込むなら、最終的にはRust側で以下のようなCLIにするのが自然です。

```bash
misaka node probe --ip <ipv4>
misaka node probe --ip <ipv4> --stake-bond <txid>:0
```

Rust実装で使える既存RPCは以下です。

- `getServerInfo`
- `getPeerAddresses`
- `getConnectedPeerInfo`
- `getStakeBond`
- `getStakeBonds`
- `getDnsConfirmation`

おすすめの実装方針です。

1. `misaka-cli/src/main.rs` の `NodeCmd` に `Probe` を追加する。
2. `misaka-cli/src/node.rs` に `probe(ctx, args)` を追加する。
3. P2P到達性は `TcpStream::connect_timeout` で見る。
4. local node側の既知peer判定は `getPeerAddresses` を使う。
5. local node側の接続中peer判定は `getConnectedPeerInfo` を使う。
6. validator判定は `--stake-bond` がある場合だけ `getStakeBond` を使う。
7. `--validator-id` を将来追加する場合は `getStakeBonds` をページングして探す。

## 安全性

このPoCは読み取り専用です。

行うことは以下だけです。

- TCP connect確認
- DNS query
- systemd status確認
- `misaka node doctor`
- `kaspa-pq-validator status`

以下は行いません。

- node再起動
- DNS seeder再起動
- validator起動
- bond作成
- 鍵読み取り
- 秘密鍵出力
