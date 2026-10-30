# PALW の役割とネットワーク範囲

## 現行ネットワーク

この Wiki で案内する現行の公開テストネットは **`testnet-12`**(R-core+、release commit `0e8ec984e`)です。testnet-11 は旧ネットワークで、現行 `main` のビルドでは参加できません。

## PALW の参加者のロールは 2 つ

| 役割 | 責務 | 採掘 | 検証 |
|---|---|---:|---:|
| **Producer / miner** | PALW の仕事を実行し、受理される block / claim を作って送る | する | 自分の生成物を確認する |
| **Panel verifier** | 割り当てられた PALW の仕事を再実行し、結果を検証して verdict を返す | しない | する |

Producer / miner と Panel verifier は別の役割です。Panel verifier は採掘をせず、Producer / miner が出した仕事の検証を担当します。

testnet-12 では、producer の node は同じ bond の panel seat の義務も常に実行します。producer の bond で verifier を別の process として起動してはいけません(1 つの bond は 1 つの process だけ。2 つ動かすと round permit に二重署名して slash されます)。

| 役割 | bond collateral |
|---|---|
| Producer / miner | 13,000 MSK 以上 |
| Panel verifier(seat) | 130,000 MSK 以上 |
| DNS finality の validator(PALW とは別の役割) | 20,000,000 MSK 以上 |

## DNS の位置づけ

DNS seeder は peer を見つけるためのネットワーク基盤です。PALW の仕事を作らず、採掘せず、Panel verifier として verdict を返すこともありません。DNS の運用と PALW の参加手順を混同しないでください。node は組み込みの seeder 名(`seeder1.misakascan.com` など)を引き、返ってきた IP の 26311 番に接続します。DNS で見つからない場合は `--addpeer=169.58.232.113:26311` を追加します。

DNS finality の validator(`misaka validator`)は DNS seeder とは別物です。testnet-12 では、validator が 6 つ以上、active stake が合計 120,000,000 MSK 以上になるまで DNS finality は Bootstrap の状態で、DNS の reorg gate は効きません(公開ノートの既知の問題 3)。その間、coinbase は 600 DAA(約 20 時間)の fallback だけで成熟します。

## 参加コマンド

Producer / miner:

```bash
misaka --network testnet-12 mining setup
misaka --network testnet-12 mining start
```

Panel verifier:

```bash
misaka --network testnet-12 verifier setup
misaka --network testnet-12 verifier start
misaka --network testnet-12 verifier status
```

node だけを運用する場合は、PALW の Bond もモデルの artifact も要りません。peer を見つけるのに DNS を使います。

## 関連ページ

- [PALW Participation](PALW-Participation-JA)
- [Testnet-12 検証参加ガイド](Testnet-12-Verification-Participation-JA)
- [Operations Notes](Operations-Notes)
