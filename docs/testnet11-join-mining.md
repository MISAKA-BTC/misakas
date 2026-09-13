# Joining Testnet-11 as a PALW producer

Verified against current `main` on 2026-09-13. Testnet-11 Relaunch 5f produces blocks with PALW ConsensusV2 at a frozen 120-second cadence. `kaspa-pq-miner` and `misaminer` cannot create the required attempt envelope; production runs inside `kaspad --palw-produce`.

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
  --rpclisten-borsh=default \
  --addpeer=169.58.39.220:26311
```

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
Consensus params fingerprint: ae1d61628da50c7becea62f0a8f08c8654d190c60b2e104df0010b121ba4d3d8 (network testnet-11)
```

## 3. Recommended setup

```bash
misaka --network testnet-11 mining setup
```

The ADR-0122 wizard checks the node, network identity, model, key, funds, Bond registry, artifact, panel capability and fee output, then writes `~/.misaka/mining.toml`. Re-running the command resumes from chain and local state.

For an existing Floor Bond:

```bash
misaka --network testnet-11 mining setup \
  --model floor \
  --key-file ~/.misaka/miner.seed \
  --bond <registered-bond-txid>:<index> \
  --peer 169.58.39.220:26311
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
  --addpeer=169.58.39.220:26311 \
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

Equivalent manual shape:

```bash
kaspad --testnet --netsuffix=11 --appdir=~/.t11 \
  --listen=0.0.0.0:26311 --rpclisten-borsh=default --utxoindex \
  --addpeer=169.58.39.220:26311 \
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
