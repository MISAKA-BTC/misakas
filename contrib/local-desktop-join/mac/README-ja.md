# MacでMISAKA testnetへローカル参加する

これはVPSを使わず、Mac上でMISAKA nodeを起動するための入口です。

安定運用はVPS版が推奨ですが、参加ハードルを下げる目的ではMacローカル起動が便利です。

## 最短

Finderでこの順に実行します。

```text
1. start-local-web-ui.command
2. ブラウザで node自動セットアップ を押す
```

初回はbuildに時間がかかります。

Web UIはVPS版と同じ画面構成に寄せています。
setup画面からnode参加を進め、Dashboardで同期とP2Pの状態を確認できます。

ブラウザタブを閉じても、`start-local-web-ui.command` をもう一度実行すれば、起動中のWeb UIを検出して同じURLを開き直します。
Terminal windowを閉じるとWeb UI自体は止まりますが、起動済みnodeは `stop-all.command` まで動き続けます。

Terminalだけで進めたい場合は、以下も使えます。

```text
start-misaka-local-node.command
check-status.command
```

エラー報告用support logを作る場合:

```bash
cd contrib/local-desktop-join
scripts/misaka-desktop-node.sh collect-support-log
```

HomebrewがないMacでも、buildに必要な `protoc` はscriptがローカルへ自動取得します。
もし別のnative library buildで失敗した場合は、Homebrewを入れてから以下を実行してください。

```bash
brew install pkg-config openssl@3 protobuf
```

## validator参加について

以前あった `prepare-validator.command`（validator key作成、funding miner、Bond作成）は、DNS-finality validator参加フローの廃止に合わせて削除しました。PALWはvalidatorを使いません。

PALWのマイニングは、このフォルダではなく `misaka` CLI の `misaka mining setup` で設定します。

## 止める

```text
stop-all.command
```

## Macのスリープについて

node起動中は、scriptがmacOS標準の `caffeinate` を使ってスリープしにくくします。

止める場合は `stop-all.command` を使います。

無効にしたい場合:

```bash
MISAKA_KEEP_AWAKE=0 scripts/misaka-desktop-node.sh start-node
```

## 注意

- Macを閉じる、再起動する、スリープするとnodeは止まります。
- 自宅回線では外部から `26211/tcp` に接続されにくいです。
- 同期はできますが、公開peerとしてはVPSより弱いです。
