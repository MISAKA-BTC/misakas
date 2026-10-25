# misakas Wiki

> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

この Wiki は、**現行 `main` と公開テストネット `testnet-12`(R-core+、ADR-0152 v3.1)** を使う人向けのガイドです。testnet-12 は release commit `0e8ec984e` から 2026-09-25/26 JST に公開されました。内容は 2026-09-25 に [公開ノート](https://github.com/MISAKA-BTC/misakas/blob/main/docs/t12-launch-2026-09-25.md)、[参加手順](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md)、`Params` と照合しました。

仕様の正本は [MISAKA-BTC/misakas](https://github.com/MISAKA-BTC/misakas) のコード、[docs](https://github.com/MISAKA-BTC/misakas/tree/main/docs)、各バイナリの `--help` です。この Wiki と食い違う場合は、node の表示と `misaka bond status` を正としてください。

> [!IMPORTANT]
> 公開前に、[testnet-12 公開ノート](https://github.com/MISAKA-BTC/misakas/blob/main/docs/t12-launch-2026-09-25.md) を必ず読んでください。リリースの中身、既知の問題(CRITICAL が 2 件あり、公開後の fence で修正)、入金を確定とみなしてよい基準が書いてあります。**testnet-12 では、block 数・blue score の深さ・DAA の差で入金を確定とみなさないでください。**

## まず読むページ

1. [Quick Start](Quick-Start) — ビルド、ノード、Bond、Floor producer。node を置かずに掘る場合は [node なしの接続先](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-node-free.md)（`169.58.232.113` の公開 wRPC）
2. [Testnet-12 Operator UI](Testnet-12-Operator-UI-JA) — セットアップウィザードと dashboard
3. [FAQ / Troubleshooting](FAQ-Troubleshooting) — よくある停止理由
4. [Operations Notes](Operations-Notes) — ポート、サービス化、更新
5. [PALW Participation](PALW-Participation-JA) — node / Producer / Panel seat
6. [検証参加ガイド](Testnet-12-Verification-Participation-JA) — panel seat の用意、起動、監視、停止
7. [Adding a Model](Adding-a-Model-JA) — モデル class と lifecycle
8. [PALW の役割とネットワーク範囲](PALW-Roles-and-Network-Scope-JA) — 公開ロールと DNS の範囲
9. [Privacy: What PALW Sees](Privacy-What-PALW-Sees) — prompt と receipt
10. [EVM Pruned Node](EVM-Pruned-Node) — EVM state の保持

## 現行ネットワーク

| 項目 | 値 |
|---|---|
| network | `testnet-12`(R-core+) |
| release commit | 現行 `main`。公開 fleet は commit `247ade1f9`(rev `247ade1f90b6`、2026-10-25) |
| consensus params fingerprint | `ef379378f09bf33f29e97854bbee157a8f86fa1dd8248e1a847f08c5c6e7f15e` |
| fence schedule | `750, 1000, 1300, 1700, 2000, 3600, 5300, 5395, 5490, 5585, 15500`(schedule id `9ef2b1a795c74d6faced545752f2d17c7e731e8e94155d078bc00cbee6c878f5`) |
| genesis | `a27f8f44fe4d91a5…`(全体は公開ノートに記載) |
| premine txid | `5e0d5f1b37a71288…`(genesis の bond と fee float はこの txid の上にある) |
| node の起動 | `kaspad --testnet --netsuffix=12` |
| node なしの採掘 | `169.58.232.113` の `wss://wallet.misakascan.com/t12/wrpc/borsh` と `169.58.39.220` の `wss://llm.misakascan.com/t12/wrpc/borsh`。手順は [testnet12-node-free.md](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-node-free.md) |
| CLI | `misaka --network testnet-12`(何も指定しないときの既定も testnet-12) |
| P2P | `26311` |
| node gRPC | `26210` |
| wRPC Borsh | `27210` |
| wRPC JSON | `28210` |
| dashboard | `127.0.0.1:8791` |
| PALW cadence | 1 block 120 秒。1 DAA = 1 execution span なので、1,000 DAA は約 33 時間 |
| Explorer | [misakascan.com](https://misakascan.com/)(testnet-12 を表示) |
| Faucet | **未定**(testnet-12 の資金はまだ入っていない) |

参加するビルドは現行 `main` です。公開 fleet と同じ consensus params fingerprint `ef379378f09bf33f…` と、上の fence schedule が起動ログに出ます。int-13 の 4 本(`palw_audit_1004_v1`、`palw_gen_range_twin_v1`、`palw_model_court_window`、`palw_receipt_spend_v4`)は DAA 15,500 で有効になります。handshake の identity は fleet と同じなので、その高さまで同じ chain に乗ったまま参加できます。

## 重要な区別

- **現行 `main` のビルドでは testnet-11 に参加できません。** testnet-11 のノードを動かし続ける場合は、旧 `main` の commit `1f98d3bf4` をビルドします(詳細は [README](https://github.com/MISAKA-BTC/misakas#readme))。
- testnet-12 は新しいチェーンです。testnet-11 の bond はありません。key file は流用できますが、bond は testnet-12 で登録し直します。
- Bond の額は mainnet 想定です: producer は **13,000 BILI 以上**、panel seat は **130,000 BILI 以上**、DNS finality の validator は **20,000,000 BILI 以上**。
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

旧ネットワーク testnet-11 向けのページ([Operator UI](Testnet-11-Operator-UI-JA)、[検証参加ガイド](Testnet-11-Verification-Participation-JA))は記録として残しています。
