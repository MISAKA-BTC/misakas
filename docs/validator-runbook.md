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
> addresses, and the PRODUCTION-policy stake-bond minimum **20,000,000 MSK = `--amount 2000000000000000`**.
> testnet also enforces **two-dimensional** finality (WorkDepth + StakeDepth), so a single
> validator confirms after ~10 attested epochs rather than instantly. Use a **fresh
> `--signed-epoch-db`** per network (the anti-equivocation guard keys on epoch numbers).

**Proven live on the activated testnet (12B premine + 18B emission tokenomics):** keygen → bond
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

### 2b. ⚠️ IMPORTANT — stop the miner before bonding (immature-UTXO trap)
A coinbase UTXO is only spendable after **coinbase maturity = 1000 DAA**. The validator's
funding scan picks the **newest** coinbase at your address — so if the miner keeps paying that
address every block, the newest UTXO is ALWAYS younger than 1000 and `bond` is rejected with:
```
Rejected ... spends an immature UTXO: coinbase ... daa N while merging daa M, maturity 1000 hasn't passed yet
```
This repeats forever no matter how long you wait. **Fix: stop minting new coins to the funding
address, then let the last one mature.**
```
# Option A (simplest): stop the miner entirely, wait ~1000 DAA, then bond.
pkill -f kaspa-pq-miner
# (the chain pauses if this is the only miner — that's fine for a bond)

# Option B (chain keeps moving): repoint the miner to a DIFFERENT address so no NEW funding
# coinbases are created, then wait ~1000 DAA past the last funding coinbase, then bond.
pkill -f kaspa-pq-miner
./target/release/kaspa-pq-miner --rpc 127.0.0.1:26610 --network-id devnet \
  --blocks 0 --min-block-interval-ms 1000 --pay-address <some_OTHER_address>
```
Confirm a mature UTXO exists (its `blockDaaScore` must be ≥ 1000 below the current virtual DAA)
before Step 3. After the bond is accepted you can resume mining to anything.

### 3. Stake the coins into a bond
```
kaspa-pq-validator bond --node-rpc 127.0.0.1:27610 --validator-key validator.seed \
                        --amount 50000000 --network devnet
```
Prints `bond_outpoint: <txid>:0`. Output-0 is the locked stake (ADR-0016 §D.1).

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
> = 0`), so any positive `--amount` works (e.g. the 2 MSK a tester used). **Mainnet/testnet require
> `--amount ≥ 20 000 000 KAS`** (`min_bond_amount_sompi` in the PRODUCTION DNS params, user decision
> 2026-06-01) — a smaller bond is rejected at acceptance and can never attest.

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
