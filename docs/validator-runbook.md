# kaspa-pq (MISAKA) Validator Participation Runbook

**Can anyone join by following steps, with NO code changes?**
**YES — on a network where the DNS overlay is activated.** The per-validator flow below uses
only the shipped binaries (`kaspad`, `kaspa-pq-validator`, `pq-miner`). Zero source edits.

The one thing that is a *network-launch* parameter (not a per-validator step): the DNS
overlay must be ACTIVE on the network — `DnsParams.dns_activation_daa_score` reached. On
mainnet/testnet/simnet today `dns_params = None` (overlay off); on the experimental devnet
it is `0` (active from genesis). Activating is a one-time launch choice, not a per-validator
action.

**Proven live 2026-05-30** on the activated devnet (binary `2125be18`): keygen → mine → bond
(active) → run → attested epochs 87/88 with the equivocation guard firing and **0
BadCoinbaseTransaction** (the reward coinbase is construction==validation on the live chain).

---

## Prerequisites
- A synced `kaspad` for your network, run with `--utxoindex` and a **borsh** wRPC port, e.g.
  `--rpclisten-borsh=127.0.0.1:27610` (the sidecar speaks borsh, not JSON).
- The `kaspa-pq-validator` binary (and `pq-miner` if you self-fund by mining).

## Steps

### 1. Generate a validator key
```
kaspa-pq-validator keygen --out validator.seed --network devnet
```
Prints `validator_id` and a `funding_address` (`misakadev:…`). Keep `validator.seed` secret
and run it on ONE host only (equivocation safety).

### 2. Get coins to the funding address
Mine to it, or send from a wallet. To mine:
```
pq-miner --rpc 127.0.0.1:26610 --network-id devnet \
         --pay-address <funding_address> --min-block-interval-ms 1000
```
Wait for coinbase maturity (1000 DAA) so the UTXO is spendable.

### 3. Stake the coins into a bond
```
kaspa-pq-validator bond --node-rpc 127.0.0.1:27610 --validator-key validator.seed \
                        --amount 50000000 --fee 20000 --network devnet
```
Prints `bond_outpoint: <txid>:0`. Output-0 is the locked stake (ADR-0016 §D.1). Pick
`--fee` ≥ the node's standard minimum (the mass-based min for the bond shape is ~14446; the
10000 default floor is too low — use 20000).

### 4. Verify the bond is active
```
kaspa-pq-validator status --node-rpc 127.0.0.1:27610 --stake-bond <txid>:0
```
Expect `bond_status: active`, `bond_amount: 50000000`.

### 5. Run the validator (attests every epoch)
```
kaspa-pq-validator run --node-rpc 127.0.0.1:27610 --validator-key validator.seed \
                       --stake-bond <txid>:0 --signed-epoch-db validator.state --network devnet
```
Logs `submitted attestation shard for epoch N` each epoch; the equivocation guard logs
`already attested epoch N (target moved); skipping` when the sink moves mid-epoch. Back up
`validator.state` (it is the cross-restart double-sign guard).

### 6. Reward
Every active bond whose attestation is included earns a stake-proportional **§E
participation** share of the per-block validator pool (25% of subsidy under §F), paid to the
validator's reward address (= the funding-address payload) in the coinbase. It accrues
automatically — no extra step.

## Notes
- `--enable-validator` on `kaspad` runs the SAME logic in-process (no separate sidecar):
  `kaspad … --enable-validator --validator-key <seed> --stake-bond <txid:0>
  --validator-mode=active`.
- Slashing is for equivocation only; the one-host rule + `validator.state` guard keep an
  honest operator safe.
- Point `--node-rpc` at the node's `--rpclisten-borsh` port, NOT the JSON port.
