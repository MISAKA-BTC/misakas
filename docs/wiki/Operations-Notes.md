# Operations Notes — testnet-12

## Network and ports

| 用途 | 値 |
|---|---|
| network | `testnet-12` |
| node flag | `--testnet --netsuffix=12` |
| P2P | `26311` |
| gRPC | `127.0.0.1:26210` |
| wRPC Borsh | `127.0.0.1:27210` |
| wRPC JSON | `127.0.0.1:28210` |
| dashboard | `127.0.0.1:8791` |
| 公開エントリポイント | `169.58.232.113:26311`(DNS seeder `seeder1.misakascan.com` と同じホスト) |

RPC は原則 loopback のままにして、リモートからの操作は SSH tunnel で行います。

## Minimal node

```bash
kaspad --testnet --netsuffix=12 --utxoindex --rpclisten-borsh=default
```

DNS seeder で peer が見つからない場合は `--addpeer=169.58.232.113:26311` を追加します。同期と監視だけの node には、model artifact、producer key、Bond は要りません。

## Identity の確認

起動ログの次の 2 行を確認します。

```text
Consensus params fingerprint: b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f (network testnet-12)
Consensus fence schedule: 1000 (schedule id 93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd)
```

2026-09-26 からの release バイナリ(`8a0810992`、x86_64 Linux、glibc 2.39 floor)の sha256:

| binary | sha256 |
|---|---|
| kaspad | `07cba17406c486e0e31d4d46e107b2998cf70229125e804513f59099051c903b` |
| misaka | `bf847721df14b7646b284cc89de21fe998315b83141b618c0fc08ba178ffb901` |
| palw-class | `0a3346be3616eb727a59f4c89e35036280a5eaa82720d6484a9e8106f89c23b1` |
| misaka-dnsseeder | `de36ec0053247c24d61e0e1924792c52d1103454265f9a91f108078730f7dfe3`(公開 fleet の DNS seeder はこの更新で入れ替えていない) |

公開時の release バイナリ(`0e8ec984e`、x86_64 Linux、glibc 2.39 floor)の sha256:

| binary | sha256 |
|---|---|
| kaspad | `5a357623c74f8e786cd244aef783855a81d8490222da15bf1a76e3dab987f149` |
| misaka | `c80e608aea340cd81eea83a503594568d9c109e271a360a40667b9f50ad4c176` |
| palw-class | `918c3554ef6ed29dff3383963e43289fc6752cf9de51a73e3dc9f2aed6a895ee` |
| misaka-dnsseeder | `8ff837efa2c8fce4bd758f5d29806af6c47ad2f80aa48649bb9f285fa68812ce` |

## Updating

1. `git pull --ff-only` のあと release build を作る。
2. 新旧のバイナリの SHA-256 を記録する。
3. node を止め、旧バイナリを名前を付けてバックアップする。
4. バイナリを入れ替え、**同じ app dir** で起動し直す。
5. fingerprint、fence schedule、peer、同期、サービスの状態を確認する。

既知の問題(公開ノート §2)は、公開後に fence またはノードの更新で直す予定です。更新の告知があったら、上の手順でバイナリを入れ替えてください。

最初の testnet-12(genesis `a8cabac4…`)や testnet-11 の datadir は genesis が違うので使えません。削除せずに別名へ退避します。バイナリを更新するだけのときは、現在の app dir をそのまま使います。

**app dir は消さないでください。** app dir には、この bond が最後に署名した round の記録(`palw-round-last-signed`)があります。消して再同期すると、まだ merge されていない permit にもう一度署名し、bond が slash される可能性があります。ホストを移すときは app dir ごと移すか、復元します。

## 1 bond = 1 process

testnet-12 の node の義務には止めるスイッチがありません。bond の key と outpoint を持って起動した `kaspad` は、どれもその bond の義務(execution lane の round block を含む)を実行します。2 つが同時に動くと、同じ round permit に 2 つの別の block で署名し、bond 全体が slash されます(`RoundPermitEquivocated`)。

次のものはすべて 2 つ目の process です。

- standby の node、別のホストにコピーした node
- 動いている node の横で起動した `kaspad --palw-register-class …`(class が登録されても終了しない)。class は `misaka model add` で登録するか、動いている node をそのフラグ付きで起動し直す
- 古いものが動いたまま、新しい app dir で起動した同じ node

node は起動時にこの規則を表示します(`PALW bond …: run it in exactly ONE process …`)。

## Service health

```bash
systemctl is-active <service>
systemctl --failed
journalctl -u <service> -n 100 --no-pager
misaka --network testnet-12 doctor
misaka --network testnet-12 mining status
```

ログでは panic のほかに、次を確認します。

- fingerprint mismatch、genesis mismatch、fork-id mismatch
- `PALW duties NOT as planned: …`(義務が予定どおり動いていない)
- `NOT PRODUCING for N min — holding: <reason>`
- `[palw-lane-watch] no PALW work block …`(heartbeat だけで時計が進んでいる)
- bond の exposure が満杯、fee UTXO、artifact mismatch
- 8k の seat の readiness: `no proof —`、`waits for a carrier slot`(公開ノートの既知の問題 6。詰まった seat は再起動で回復させる)

## Resource profiles

`kaspad --help` の `--node-profile` が現行の入口です。

- `full`
- `bootstrap-pruned`
- `recovery-sync`
- `validator`
- `archive`
- `public-rpc`

profile が拒否する組み合わせを、手動のフラグで回避しないでください。model class の producer や seat には、artifact そのものの RAM、ディスク、実行時間が必要です(8k は `--palw-host-memory-share` 3.5 GiB 以上)。Floor はモデルのダウンロードが要りません。

## Keys and state

- producer と panel seat の key と設定をバックアップする。
- secret seed を CLI の引数やログに貼らない。
- 同じ validator seed を 2 か所で起動しない。同じ bond を 2 つの process で動かさない。
- `~/.misaka/mining.toml` と app dir の役割を混同しない。
- Bond の outpoint、fee outpoint、class artifact の hash を記録する。genesis の bond と fee float は premine txid `5e0d5f1b…` の上にある(index は bond 0〜7、fee float 41〜48)。

## Graceful maintenance

producer を止める前に:

```bash
misaka --network testnet-12 mining stop --drain
```

panel や court の義務がある場合は、drain が終わるまで待ってからサービスを止めます。testnet-12 では、消えた producer は課金されます(`ProducerWithholding` の void と 2 回目の `ReceiptTimeout`)。
