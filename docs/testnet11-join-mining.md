# Joining testnet-11 as a miner

This is the path a node that is on **no genesis registry** takes to produce blocks. Every step
below was run or read against the live network on 2026-08-24; where a number is a chain fact it
says where it comes from.

Read [§0](#0-what-you-cannot-do) first. Two of the three things people try do not work on this
network, and both fail in ways that look like something else.

---

## 0. What you cannot do

**A hash miner cannot mine this network, and neither can `misaminer`.** Every block declares PoW
algo **6** (`POW_ALGO_ID_PALW_COMMITTED_V2`), and an algo-6 header must carry a signed
`PalwAttemptEnvelopeV2` in `palw_commitment` or it fails verification with `PalwV2AttemptMissing`.
Building one means running the class's model, committing to the trace, and signing with a bonded
key — none of which a `getBlockTemplate` → search → `submitBlock` client can do. `misaminer` knows
this and stops with a message saying so; it does **not** search a target it cannot win. (It used to,
which cost one operator four hours at 400 % CPU and zero blocks.)

**Blocks are produced by `kaspad --palw-produce`.** There is no external miner client for this
network. That is not a gap in the tooling — the nonce is won by inference, so the thing that runs
the model is the thing that makes the block.

**You cannot mine without a bond.** `ready_to_produce` refuses with *"the named bond is not
registered on this chain"*. Until 2026-08-24 the only bonds any chain had were the ones its genesis
registry named — six, on testnet-11 — so this document could not have been written. §3 is the step
that changed.

---

## 1. What you need

| | |
|---|---|
| the node | `kaspad` from this repo, built with `--release` |
| a key | a 32-byte ML-DSA-87 seed — `misaka key gen` (§2) |
| MSK | enough to cover the collateral plus a transaction fee, in a **non-coinbase** output (§2) |
| a model | **no.** The default class is the integer floor; see §5 |

`--netsuffix=11`, P2P **26311**, RPC **26312**. A node on the right chain logs

```
Consensus params fingerprint: 048e69026e559e67584ded64f1b6279148e3459975ef9d710e029eaaed638ee0 (network testnet-11)
```

A different fingerprint means a different ruleset, and the two will refuse each other at handshake.
Do not treat that as a connectivity problem.

---

## 2. Key, address, funds

```bash
cargo build --release -p misaka-cli --bin misaka
./target/release/misaka key gen --out ~/.misaka/miner.seed
./target/release/misaka key address --key-file ~/.misaka/miner.seed
```

(The crate is `misaka-cli`; the binary it installs is `misaka`. `key gen` prints the address too, so
the second command is only for looking it up again later.)

The address is ML-DSA-87 P2PKH (`misakatest:…`). It is where rewards are paid, and where the
collateral returns if the bond is ever retired — the registration names one payee for both.

**Fund it with a normal transfer, not with mining rewards.** Two separate rules bite a coinbase
output: `coinbase_maturity`, and the ADR-0018 DNS settlement floor
(`coinbase_settlement_long_maturity_daa` = 600 on testnet-11). On top of that the node's funding
scan skips coinbase entries outright, so a coinbase UTXO will not be found at all and the only
symptom is "no confirmed UTXO to spend".

---

## 3. Register a bond

```bash
kaspad --testnet --netsuffix=11 --appdir=~/.t11 \
  --listen=0.0.0.0:26311 --rpclisten=127.0.0.1:26312 \
  --addpeer=169.58.39.220:26311 \
  --palw-register-bond \
  --palw-producer-key=~/.misaka/miner.seed \
  --palw-producer-pay-address=<your misakatest: address>
```

The node waits until it is synced, builds one `BondRegistered`, and submits it in a transaction that
**locks the collateral in its own output**. Then it prints the line you need:

```
[palw-panel] registered bond <txid>:0 with <n> sompi of collateral.
Restart with --palw-producer-bond=<txid>:0 (and --palw-produce) to mine with it
```

**That line is the only place the bond's outpoint appears.** It is this transaction's own id, which
did not exist until the transaction was built — nobody can tell it to you in advance, and the node
does not store it anywhere else. Keep it.

If it cannot proceed it says why, once per reason rather than every five seconds. The usual reason
is that no confirmed non-coinbase UTXO is visible yet; fund the address and it picks it up without a
restart.

### Collateral

`--palw-bond-collateral` is optional and **the default is not the chain's minimum**. A bond may hold
a claim only while

```
reserved_exposure + claim_exposure  ≤  collateral × max_exposure_ratio_permille / 1000
```

and one claim costs `pwu × slash_value_per_pwu`, where `pwu` rises as the class retargets. The
chain's floor (400,000 sompi, `min_collateral_sompi`) therefore buys a bond that may not fit a
**single** claim — and that producer holds forever, having locked real money to get there. The node
reads the current numbers off the chain and sizes for one claim, logging both. Passing a smaller
value is allowed and warned about.

---

## 4. Produce

```bash
kaspad --testnet --netsuffix=11 --appdir=~/.t11 \
  --listen=0.0.0.0:26311 --rpclisten=127.0.0.1:26312 \
  --addpeer=169.58.39.220:26311 \
  --palw-produce --palw-panel \
  --palw-producer-key=~/.misaka/miner.seed \
  --palw-producer-bond=<txid>:0 \
  --palw-producer-pay-address=<your misakatest: address>
```

All four of key, bond, pay address and a class are required or the producer does not start at all.

When it holds instead of producing, the reason carries its numbers:

```
[palw-producer] holding: <reason> [class=… epoch=… produced=… budget=… exposure=…/… per_claim=…]
```

Those are worth reading rather than skimming — `this class's epoch budget is already spent` is what
an exhausted cap says **and** what a class that was never granted one says, and the numbers are how
you tell them apart (`budget=0` is the second).

---

## 5. Which class you are mining

Omitting `--palw-producer-class` mines `bundle.base_class_id`, which on testnet-11 is the **BASE-0
floor** — `c185df95…c654a`, a deterministic-integer class whose artifact is derived from a seed on
every node. **No GGUF, no download, no `--palw-metal-worker`.** The floor is also exempt from the
per-class epoch budget, so it is the one class that can always produce.

To mine a registered model class instead, pass its id with `--palw-producer-class` and give the node
the runtime that class pins (`--palw-metal-worker`). A class registered while an epoch is already
running has share from the moment it activates.

---

## 6. What a bond costs you

The collateral is locked in the output the registration names and is reclaimable at your pay
address once the bond is retired (an owner ML-DSA-87 signature over the bond key releases it). It is
also what a court can slash if this node commits a provably wrong execution — that is the whole
point of it, and it is why the exposure ceiling exists.
