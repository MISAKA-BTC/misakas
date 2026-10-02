# misakas Wiki

この Wiki は、**現行 `main` と公開テストネット `testnet-12`(R-core+、ADR-0152 v3.1)** を使う人向けのガイドです。testnet-12 は release commit `0e8ec984e` から 2026-09-25/26 JST に公開され、その後の post-launch fence で更新されています。

仕様の正本は [MISAKA-BTC/misakas](https://github.com/MISAKA-BTC/misakas) のコード、[docs](https://github.com/MISAKA-BTC/misakas/tree/main/docs)、各バイナリの `--help` です。producer の詳しい手順の正本は [testnet12-join-mining.md](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md) です。この Wiki と食い違う場合は、node の表示と `misaka bond status` を正としてください。

> [!IMPORTANT]
> 参加する前に、[testnet-12 公開ノート](https://github.com/MISAKA-BTC/misakas/blob/main/docs/t12-launch-2026-09-25.md) を必ず読んでください。リリースの中身、post-launch fence、既知の問題、入金を確定とみなしてよい基準が書いてあります。**testnet-12 では、block 数・blue score の深さ・DAA の差で入金を確定とみなさないでください。**

## まず読むページ

1. [Quick Start](Quick-Start) — ビルド、ノード、Bond、Floor / 8k producer、状態の確認、dashboard、停止
2. [FAQ / Troubleshooting](FAQ-Troubleshooting) — よくある停止理由
3. [Operations Notes](Operations-Notes) — ポート、更新、サービス化、1 bond = 1 process
4. [PALW Participation](PALW-Participation-JA) — ロール、model class、Bond と exposure、課金、報酬、DNS の位置づけ
5. [検証参加ガイド](Testnet-12-Verification-Participation-JA) — panel seat の用意、起動、監視、停止
6. [Adding a Model](Adding-a-Model-JA) — モデル class と lifecycle
7. [Privacy: What PALW Sees](Privacy-What-PALW-Sees) — prompt と receipt
8. [EVM Pruned Node](EVM-Pruned-Node) — EVM state の保持

## 現行ネットワーク

| 項目 | 値 |
|---|---|
| network | `testnet-12`(R-core+) |
| consensus params fingerprint | `254509533bb693ced0fed823a4c25e166ba2542d576e4021b0e4b4d6fe4079e1` |
| fence schedule | `750, 1000, 1300, 1700, 2000, 3600`(schedule id `1e39c738b97a695c8a2c2d4129660eda8fa7ac5f1e8b529b916314c01750c593`) |
| genesis | `a27f8f44fe4d91a5…`(全体はリポジトリの `release.json`) |
| premine txid | `5e0d5f1b37a71288…`(genesis の bond と fee float はこの txid の上にある) |
| node の起動 | `kaspad --testnet --netsuffix=12` |
| CLI | `misaka --network testnet-12`(何も指定しないときの既定も testnet-12) |
| ポート | [Operations Notes](Operations-Notes#network-and-ports) |
| PALW cadence | 1 block 120 秒。1 DAA = 1 execution span なので、1,000 DAA は約 33 時間 |
| Explorer | [misakascan.com](https://misakascan.com/)(testnet-12 を表示) |
| Faucet | **未定**(testnet-12 の資金はまだ入っていない) |

識別値の正本はリポジトリ直下の `release.json` です。上の値は 2026-10-02 時点のもので、post-launch fence が追加されると fingerprint と schedule id が変わります。古いビルドのノードは、まだ持っていない fence の高さから handshake で拒否されるので、告知があったら `main` から再ビルドしてください([Operations Notes](Operations-Notes#updating))。

## 重要な区別

- testnet-12 は新しいチェーンです。旧ネットワークの bond はありません。key file は流用できますが、bond は testnet-12 で登録し直します。
- Bond の額は mainnet 想定です: producer は **13,000 MSK 以上**、panel seat は **130,000 MSK 以上**、DNS finality の validator は **20,000,000 MSK 以上**。
- Bond の UTXO が locked でも、registry に正式登録済みとは限りません(`misaka bond status` で確認します)。
- 1 つの key で登録できる bond は、チェーンの存続期間を通じて 1 つだけです。登録後に collateral を追加することもできません。
- **1 つの bond は 1 つの process だけで動かしてください。** 同じ bond の key と outpoint で `kaspad` を 2 つ動かすと round permit に二重署名し、bond 全体が slash されます(`RoundPermitEquivocated`)。
- node の義務(panel seat の duty、execution lane の round block、chain classes)は起動オプションに関係なく常に動きます。`--palw-panel` と `--palw-round-lane` は受け付けますが何もせず、警告を出すだけです。
- BASE-0/Floor はモデルファイル不要です。8k の Qwen2.5 class は artifact と 3.5 GiB 以上のメモリが必要です。2M class は公開時点では閉じています。
- 検証ルール S1 / S2 / S3 は DAA 0 から有効です。別プロセスやフラグはありません。

## 公式リンク

- [Source](https://github.com/MISAKA-BTC/misakas)
- [testnet-12 公開ノート](https://github.com/MISAKA-BTC/misakas/blob/main/docs/t12-launch-2026-09-25.md)
- [Testnet-12 参加手順(producer)](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md)
- [testnet-12 regenesis の記録](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet-12-regenesis-2026-09-23.md)
- [Validator runbook](https://github.com/MISAKA-BTC/misakas/blob/main/docs/validator-runbook.md)
- [Explorer](https://misakascan.com/)
- [Releases](https://github.com/MISAKA-BTC/misakas/releases)
