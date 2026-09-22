# Joining Testnet-11 as a PALW producer

Verified against current `main` on 2026-09-22. Testnet-11 Relaunch 5f produces blocks with PALW ConsensusV2 at a frozen 120-second cadence. `kaspa-pq-miner` and `misaminer` cannot create the required attempt envelope; production runs inside `kaspad --palw-produce`.

## 1. Build

```bash
git switch main
git pull --ff-only
cargo build --release -p kaspad -p misaka-cli
```

The binary from `misaka-cli` is `target/release/misaka`.

## 2. Start or reach a node

```bash
kaspad --testnet --netsuffix=11 --utxoindex \
  --rpclisten-borsh=default
```

DNS seeders provide the current testnet-11 bootstrap peers. If an environment requires an
explicit bootstrap, resolve a seeder for that invocation and pass the resulting IP to `--peer`;
the flag currently accepts IP addresses, not hostnames.

Default ports:

| purpose | port |
|---|---:|
| P2P | 26311 |
| node gRPC | 26210 |
| wRPC Borsh | 27210 |
| wRPC JSON | 28210 |

`misaka`, the wallet and the validator use wRPC Borsh. If the node uses a custom listener, pass its actual address with `--rpc`.

The startup log must report:

```text
Consensus params fingerprint: 79b49c238c46b0d97ab9b46d79fd5f85f8b50da623921a53f0af361515d50640 (network testnet-11)
```

## 3. Recommended setup

```bash
misaka --network testnet-11 mining setup
```

The ADR-0122 wizard checks the node, network identity, model, key, funds, Bond registry, artifact, panel capability and fee output, then writes `~/.misaka/mining.toml`. Re-running the command resumes from chain and local state.

Large class artifacts do not need to be copied into the node directory. Point setup at the exact
file, or at a mounted directory containing it:

```bash
export MISAKA_PALW_ARTIFACT=/srv/misaka/palw/qwen25-1.5b-a16-2m.palwart
# alternatively: export MISAKA_PALW_ARTIFACT_DIRS=/srv/misaka/palw:/mnt/models
misaka --network testnet-11 mining setup --model <class-id-or-name> --verify-artifact
```

For every non-Floor class, setup reads the artifact and matches its computed PALW state root to
the class registered on chain before it writes `mining.toml`. `--verify-artifact` is retained as a
compatibility/documentation flag; the root check is now mandatory, so an old or mismatched file
cannot become a later panel-start failure.

For an existing Floor Bond:

```bash
misaka --network testnet-11 mining setup \
  --model floor \
  --key-file ~/.misaka/miner.seed \
  --bond <registered-bond-txid>:<index>
```

## 4. Inspect an existing Bond

```bash
misaka --network testnet-11 bond status --bond <txid>:<index>
```

This exact-outpoint mode is read-only and needs no key. `--outpoint` and `--carrier` are aliases. When `--class-id` is omitted the network's BASE-0/Floor class is used for the internal lookup and sizing output.

- `registry: REGISTERED`: formally registered PALW Bond.
- `registry: NOT REGISTERED`: an ordinary/reserved/locked UTXO, not a registry record.
- `UNDERSIZED`: registered, but insufficient for sustained production in the inspected class.

Registration is append-only: one key can register one Bond for the life of the chain. Retirement does not make the key reusable. Collateral cannot be topped up. For more sustained capacity, create a new key, fund it, and register a new correctly sized Bond.

## 5. Register only when no Bond exists

The wizard performs registration after showing the spend and asking for confirmation. The manual one-shot form is:

```bash
kaspad --testnet --netsuffix=11 --utxoindex \
  --rpclisten-borsh=default \
  --palw-register-bond \
  --palw-producer-key=~/.misaka/miner.seed
```

Do not add `--palw-register-bond` when `bond status` says `REGISTERED`.

Funding must be a mature, non-coinbase output. Registration spends one input, locks the collateral and leaves change/fee capacity. Keep the printed Bond outpoint.

### Collateral

The node derives whole-claim-lifetime collateral from the selected class's current PWU and windows. Do not copy an amount from an old relaunch.

For the current Floor profile the derived amount is:

```text
1,110,106,160 sompi = 11.10106160 MSK
```

The funding output needs additional room for fee/change. Model classes are materially larger; use `misaka mining setup` or `misaka bond status --class-id ...` for the current amount.

## 6. Start Floor production

```bash
misaka --network testnet-11 mining start --print-command
misaka --network testnet-11 mining start
```

S1（DAA 7,200）以降、この起動が capture から決定論的 checkpoint `SC01` を公開します。checkpoint 用の別フラグはありません。panel の partial 席が区間 resume するため、producer も検証席と同じ世代のバイナリを使い、receipt 期限まで capture を保持してください。手順の本体は [検証参加ガイド](testnet11-verification-participation-ja.md) です。

Equivalent manual shape:

```bash
kaspad --testnet --netsuffix=11 --appdir=~/.t11 \
  --listen=0.0.0.0:26311 --rpclisten-borsh=default --utxoindex \
  --palw-produce --palw-panel \
  --palw-producer-key=~/.misaka/miner.seed \
  --palw-producer-bond=<registered-bond-txid>:<index> \
  --palw-fee-outpoint=<mature-non-bond-txid>:<index>
```

For Floor, omit `--palw-producer-class` and `--palw-class-artifact`. The fee outpoint must be different from the Bond collateral outpoint.

## 7. Observe

```bash
misaka --network testnet-11 mining status --watch 5
misaka --network testnet-11 doctor
misaka --network testnet-11 work list
misaka --network testnet-11 rewards
```

The read-only dashboard:

```bash
misaka --network testnet-11 dashboard --listen 127.0.0.1:8791
```

## 8. Stop safely

```bash
misaka --network testnet-11 mining stop --drain
```

Drain stops new work and keeps the process available for open claim/panel/court duties. `--force` is an emergency option and can abandon responsibilities.

## 9. Epoch budgets

Class budgets remain isolated under the shipped Testnet-11 preset. ADR-0123's progressive release implementation exists in the codebase, but `palw_epoch_budget_release` is `None` on every shipped preset. Documentation must not describe it as active before a deliberate network activation.
