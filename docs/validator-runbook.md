# kaspa-pq (MISAKA) Validator Participation Runbook

**Can anyone join by following steps, with NO code changes?**
**YES — on a network where the DNS overlay is activated.** The per-validator flow below uses
only the shipped binaries (`kaspad`, `kaspa-pq-validator`, `kaspa-pq-miner`). Zero source edits.

The DNS overlay is now **active from genesis on every defined network**
(`dns_activation_daa_score = 0`). Activating is a one-time launch choice, not a per-validator
action.

> **Live network = `testnet-10`** (explorer: [misakascan.com](https://misakascan.com)). The
> command examples below were written for the earlier `devnet`; for the live testnet substitute
> **`--network testnet-10`** (or `--network-id testnet-10` for the miner), **`misakatest:`**
> addresses, the **testnet RPC ports** (gRPC `26210`, wRPC Borsh `27210`, wRPC JSON `28210` — the
> `26610/27610/28610` below are devnet defaults; on either network you can just pass
> `--rpclisten-borsh=default`), and the testnet stake-bond minimum **10 MSK = `--amount 1000000000`** (lowered from
> mainnet's 20,000,000 MSK so a tester can mine a bondable amount in seconds — see Step 3;
> **mainnet keeps 20M**). `bond` aggregates several mature coinbase UTXOs, so the
> ~3.7-MSK-per-block mining fragments no longer need manual consolidation. testnet also enforces
> **two-dimensional** finality (WorkDepth + StakeDepth), so a single validator confirms after ~10
> attested epochs rather than instantly. Use a **fresh `--signed-epoch-db`** per network (the
> anti-equivocation guard keys on epoch numbers).

**Proven live on the activated testnet (15B premine + 15B emission tokenomics):** keygen → bond
(20M MSK from the premine) → run → attests every epoch, `dnsConfirmed: true` with the
equivocation guard firing and **0 BadCoinbaseTransaction** (the reward coinbase is
construction==validation on the live chain). The same flow was first proven on devnet (2026-05-30).

---

## Prerequisites
- A synced `kaspad` for your network, run with `--utxoindex` and a **borsh** wRPC port, e.g.
  `--rpclisten-borsh=0.0.0.0:27610` (the sidecar speaks borsh, not JSON).
- The `kaspa-pq-validator` binary (and `kaspa-pq-miner` if you self-fund by mining). All three
  binaries are produced by `cargo build --release` into `./target/release/` (or download a
  release from the repo). Commands below assume they are on your `PATH` or prefix them with
  `./target/release/`.

## Step 0. Start the node and a miner

These MUST be running before keygen/bond. Run each in its own terminal (or detached).

**0a. Start `kaspad`** — `--utxoindex` is REQUIRED (the validator scans your funding UTXOs via
it); the borsh port `:27610` is what the validator connects to; the JSON port `:28610` is for
explorers/wallets (optional but handy).
```
./target/release/kaspad --devnet --utxoindex \
  --rpclisten=127.0.0.1:26610 \
  --rpclisten-borsh=0.0.0.0:27610 \
  --rpclisten-json=0.0.0.0:28610 \
  --appdir ~/.kaspa-pq-devnet
```
Add `--connect=<seed-ip>:<p2p-port>` to join an existing mesh (P2P port is **26211 for
testnet-10**, 26611 for devnet, 26111 for mainnet — e.g. the public testnet bootstrap is
`--addpeer=95.111.236.186:26211`), or `--nodnsseed --disable-upnp --enable-unsynced-mining` for a
fresh local chain. Wait until it reports `IBD ... finished` / the chain stops advancing during sync
before proceeding (`isSynced: true`).
> If you sit at `has 0/8 outgoing P2P connections` even though the DNS seeders return addresses, the
> usual cause is a **P2P-port mismatch**: DNS returns only IPs and the node dials them on the
> network's *default* P2P port, so a peer listening on a non-default port is unreachable by
> discovery — bootstrap with an explicit `--addpeer=<ip>:26211`.

**0b. Start the miner** — note the binary is **`kaspa-pq-miner`** (NOT `pq-miner`). For now
mine to ANY address just to grow the chain; Step 2 switches it to your funding address. `--rpc`
points at the node's **grpc** port `:26610`.
```
./target/release/kaspa-pq-miner --rpc 127.0.0.1:26610 --network-id devnet \
  --blocks 0 --min-block-interval-ms 1000 --pay-address <some_address>
```
(`--blocks 0` = mine forever. `--min-block-interval-ms 1000` ≈ 1 block/s; do not set it too low
on a small mesh or you get GHOSTDAG reorgs.)

## Steps

### 1. Generate a validator key
```
kaspa-pq-validator keygen --out validator.seed --network devnet
```
Prints `validator_id` and a `funding_address` (`misakadev:…`). Keep `validator.seed` secret
and run it on ONE host only (equivocation safety).

### 2. Get coins to the funding address
Point the miner at your funding address (restart the Step-0b miner with `--pay-address
<funding_address>`), or send coins from a wallet:
```
pkill -f kaspa-pq-miner   # stop the Step-0b miner
./target/release/kaspa-pq-miner --rpc 127.0.0.1:26610 --network-id devnet \
  --blocks 0 --min-block-interval-ms 1000 --pay-address <funding_address>
```

### 2b. Wait for coinbase maturity (no need to stop the miner)
A coinbase UTXO is only spendable after **coinbase maturity = 1000 DAA**. `bond` (1) filters out
immature coinbases and (2) **aggregates the largest *mature* UTXOs** until they cover
`--amount + fee`, so you can leave the miner running — it skips the fresh immature coinbases and
sums the older mature ones. (This replaces the old single-UTXO behavior, which always picked the
newest = immature coinbase and was rejected with `spends an immature UTXO … maturity 1000 hasn't
passed yet`; consolidation is no longer required.)

Just give the first batch of coinbases time to mature: mine for a short while, then wait until the
virtual DAA is ≥ 1000 past those blocks (≈ a few minutes on the live testnet). At the testnet
subsidy (~2.5–3.7 MSK/block, decaying over time) a 10-MSK bond needs only ~4–5 mature coinbases; a
larger `--amount` needs proportionally more. The bond tx aggregates **up to 20 inputs** (to stay
within the block mass limit), so a single bond tops out at ≈ 20 × the per-block subsidy (≈ **50
MSK** at the current rate); for a larger stake, bond again (run a second `bond`) or lower
`--amount`. If `bond` reports `not enough MATURE funding … have X sompi across N mature UTXO(s)
(cap 20)`, either mine more / wait longer for maturity, or — if you've already hit the 20-input
cap — lower `--amount`. (Verified live 2026-06-08: a 10-MSK bond aggregated 5 mature coinbase UTXOs
and was accepted; the 5-input ML-DSA signing validates through consensus.)

### 3. Stake the coins into a bond
```
# devnet/simnet (no per-bond minimum): any positive amount, e.g. 0.5 MSK
kaspa-pq-validator bond --node-rpc 127.0.0.1:27610 --validator-key validator.seed \
                        --amount 50000000 --network devnet
# testnet-10 (min 10 MSK): e.g. bond 10 MSK
kaspa-pq-validator bond --node-rpc 127.0.0.1:27610 --validator-key validator.seed \
                        --amount 1000000000 --network testnet-10
```
Prints `bond_outpoint: <txid>:0` (and `funding bond from N mature UTXO(s) …` showing how many
coinbase fragments were aggregated). Output-0 is the locked stake (ADR-0016 §D.1).

**Omit `--fee` — it is auto-computed (mass-based).** A StakeBond carries the 2592-byte ML-DSA-87
public key, so its compute mass is large and the mempool's minimum relay fee is **10× the compute
mass** (`PQ_PRODUCTION_MINIMUM_RELAY_TRANSACTION_FEE` = 10 000 sompi/kg): a bond needs **≈ 270 000
sompi**, computed from the node's mass params. A manually-passed `--fee 30000` is therefore
**rejected** with `fees … under the required amount of ≈218120` — pass nothing and the validator
sizes the fee itself. (The `ATTESTATION_TX_FEE_FLOOR_SOMPI` safety floor is **250 000**, raised from
a flat 30 000 that sat below the mempool minimum and wedged any path that fell back to it.) Use
`--fee <sompi>` only to override (e.g. bump under congestion). The auto-fee logic is
network-independent (same relay rate + bond shape), so it works unchanged on testnet/mainnet.

> **Bond amount differs by network.** Devnet/simnet have no per-bond minimum (`min_bond_amount_sompi
> = 0`), so any positive `--amount` works. **testnet-10 requires `--amount ≥ 10 MSK`
> (`= 1000000000 sompi`)** — lowered from mainnet's floor (`TESTNET_DNS_PARAMS`, kaspa-pq Phase 2)
> so testers can mine a bondable amount in seconds. **Mainnet requires `--amount ≥ 20 000 000 KAS`**
> (`min_bond_amount_sompi` in `PRODUCTION_DNS_PARAMS`, user decision 2026-06-01). A smaller bond is
> rejected at acceptance and can never attest.

### 4. Verify the bond is active
```
kaspa-pq-validator status --node-rpc 127.0.0.1:27610 --stake-bond <txid>:0
```
Expect `bond_status: active`, `bond_amount: 50000000`. `status` also reports the network's DNS
finality state — `dns_confirmed`/`pow_confirmed`, the work/stake depths vs their required floors,
`dns_health` (Active / Degraded…), and `dns_anchor` (the last DNS-confirmed canonical lagged
anchor + its DAA score). Once your validator (plus the rest of the active stake) attests,
`dns_confirmed: true` with a recent `dns_anchor` is the end-to-end health signal.

### 5. Run the validator (attests every epoch)
```
kaspa-pq-validator run --node-rpc 127.0.0.1:27610 --validator-key validator.seed \
                       --stake-bond <txid>:0 --signed-epoch-db validator.state --network devnet
```
Logs `submitted attestation shard for epoch N` each epoch; the equivocation guard logs
`already attested epoch N (target moved); skipping` when the sink moves mid-epoch. Back up
`validator.state` (it is the cross-restart double-sign guard).

**Attestation fee is auto-computed (mass-based), same as `bond`/`unbond`.** Each shard carries a
4627-byte ML-DSA-87 signature, so its mempool minimum is **≈ 232 600 sompi** — the validator sizes
the fee from the node's mass params (**≈ 290 000 sompi**) at startup and logs it
(`fee … sompi, mass-based`). Omit `--fee` to auto-size; pass `--fee <sompi>` only to override (e.g.
bump under congestion). A node whose attestations are rejected with `fees 30000 … under the required
amount of 232600` is running a **pre-fix binary** that pinned the flat 30 000 floor — rebuild from
this revision (the floor is now 250 000 and `run` auto-sizes the fee) to clear the deadlock.

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
