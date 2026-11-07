# Run a validator

> **整理前の保存版。** 当時のcurrent / live / PASS・数値・手順を現在の保証にしません。新しい入口は [README](../../../../README.md)。元バイトはoriginal-markdown.zipに保持しています。

> 旧ルート README（2026-10-12 まで）から移した手順です。例に testnet-11 や古い値が含まれる場合があります。現行ネットワーク（testnet-12）の値は [README](../../README.md) と [docs/README](../../../../README.md) を正とし、`--help` と chain RPC で確認してください。

## Running a validator (testnet)

The `kaspa-pq-validator` sidecar connects to a local node over wRPC and attests while its ML-DSA-87 stake bond is active. See [docs/validator-runbook.md](../../../operations/dns-validator/validator-runbook.md). Quickstart:

```bash
# 1. generate a validator key + print its funding address

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。

[新方針・廃止範囲](../adr/0177-model-bond-allocation-without-availability-consensus.md) / [文書チェックと未完了事項](../adr/evidence/0177-model-distribution-policy-alignment-2026-10-10.md)。

kaspa-pq-validator keygen --out val.seed --network testnet
# 2. send funds to the printed funding address (mine to it, or transfer from another wallet)
# 3. stake a DNS-finality bond. Testnet-11's minimum is 10 MSK = 1,000,000,000 sompi.
#    Omit --fee to auto-size it (mass-based; the flat floor is too low for the 2592-byte pubkey).
kaspa-pq-validator bond --node-rpc 127.0.0.1:27210 --validator-key val.seed \
  --amount 1000000000 --network testnet-11
# 4. run the validator daemon (attests every epoch while the bond is active, and precommits
#    where the network schedules ADR-0128's BFT gate — testnet-11 from DAA 7,101)
kaspa-pq-validator run --node-rpc 127.0.0.1:27210 --validator-key val.seed \
  --stake-bond <txid:index> --signed-epoch-db val.state --network testnet-11 --attest-poll-secs 3
```

> Note: the funding/`run`/`bond` subcommands want the **full** network id (`testnet-11`); `keygen`'s `--network` takes the short form (`testnet`). Use a **fresh** `--signed-epoch-db` per network — reusing one across networks trips the anti-equivocation guard on overlapping epoch numbers.

The validator attests the one current canonical-ready epoch per round; the poll cadence defaults to
**3 s**. Testnet-11 is not a 10-BPS network: PALW cadence is 120 seconds per block and its DNS
attestation epoch is rescaled to 2 blue-score. Keep the generated/default poll interval unless the
current validator runbook and `--help` say otherwise.

Once enough active stake has attested across the recent epochs, `getDnsConfirmation` reports
`dnsConfirmed: true` plus a `lastDnsConfirmedAnchor` (the stake-confirmed finality point — treat
this as DNS-final, not the pov-dependent `blockHash` sink). Below DAA 7,101, confirmation on
Testnet-11 is two-dimensional: it requires both anchor-relative `WorkDepth` and `StakeDepth` under the
live network parameters. **From DAA 7,101 it is a vote** (ADR-0128): the confirmed anchor is the newest
epoch anchor that validators holding more than two thirds of the counted bonded stake have attested
and precommitted to, voting power is the bond amount, a bond silent for 5,040 DAA (about seven days)
stops counting until it attests again, and the DNS stake reorg gate refuses chains that abandon the
confirmed anchor until it goes stale. The sidecar and the in-node validator precommit by themselves
from `getPrecommitDuty` and keep a second safety log, `<signed-epoch-db>.precommits.json` — back it up
with the seed. Validators are paid 20 % of each block's subsidy from DAA 7,101 (30 % before). PALW does
not wait for any of this: its payments settle on PALW anchors (`misaka palw settlement`). The current experimental mesh permits one active validator, but the 10 BILI minimum
does not bypass the work-depth, anchor-attester or freshness checks. Per-block finality is queryable:
`getDnsConfirmation` accepts an optional `blockHash` and answers whether that block is DNS-final
(`blockIsDnsFinal` / `blockIsConfirmedAnchor`); the explorer's **DNS Finality** page lists the
confirmed chain in order.

### Remote signer / HSM (optional, ADR-0015)

`kaspa-pq-signer` is a standalone daemon that holds the ML-DSA-87 validator key **outside** the validator process and answers sign requests over a `0700` (owner-only) Unix domain socket, enforcing a signing policy (`permissive` / `audit-only` / `strict`), a `strict`-policy anti-equivocation guard (backed by a crash-consistent `SignedEpochStore`), and a tamper-evident hash-chained audit log. A compromised validator node then cannot exfiltrate the key or double-sign.

```bash
kaspa-pq-signer --socket /run/kaspa-pq-signer.sock --key val.seed \
  --state-dir ./kpq-signer-state --policy strict
```

This is a **software** signer; a hardware-HSM / PKCS#11 backend and HA failover are out of scope (see [docs/adr/0015-remote-signer-hsm-protocol.md](../../../../adr/0015-remote-signer-hsm-protocol.md)). The local key-file signer used by `kaspa-pq-validator` above remains the default.

<details>
<summary>Using a configuration file</summary>

```bash
cargo run --release --bin kaspad -- --configfile /path/to/configfile.toml   # or -C /path/...
```
The config file is a list of `<CLI argument> = <value>` lines. Pass `--help` to view all arguments:
```bash
cargo run --release --bin kaspad -- --help
```
</details>

<details>
<summary>wRPC</summary>

The wRPC subsystem is disabled by default in `kaspad` and is enabled via `--rpclisten-json=<interface:port>` (or `=default`) and `--rpclisten-borsh=<interface:port>` (or `=default`). It is a WebSocket-framed RPC supporting [Borsh](https://borsh.io/) (inter-process; client and server must be built from the same codebase) and JSON (data-structure-version-agnostic; connect with any WebSocket library) encodings.
</details>
