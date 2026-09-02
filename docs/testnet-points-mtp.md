# MISAKA Testnet Points (MTP) — how to take part, and how to check your points

> **Testnet only, and worth nothing.** MTP points are a record of testnet participation. They are
> **not** a token, not a balance, not tradable, and carry **no monetary value**. `testnet-10` MSK is
> likewise valueless test currency. There is no mainnet, no sale, and no promise that points convert
> into anything. Anyone offering to buy or sell MISAKA points or testnet MSK is running a scam.

Points are a mirror of **ML-DSA-87-signed epoch ledgers**. Everything below is checkable offline —
you never have to trust the server that serves the numbers. The scored network is **`testnet-10`**.

- [1. What earns points today](#1-what-earns-points-today)
- [2. Taking part](#2-taking-part)
- [3. Checking your points](#3-checking-your-points)
- [4. Verifying a ledger yourself](#4-verifying-a-ledger-yourself)
- [5. Epochs — when points start counting](#5-epochs--when-points-start-counting)
- [6. Current status](#6-current-status)
- [7. FAQ](#7-faq)

---

## 1. What earns points today

Two categories accrue **automatically from the chain** on `testnet-10`. No registration, no account,
no sign-up: an hourly operator-side job scans the canonical chain and credits well-formed
`misakatest:` addresses for what they did.

| | Category | Earned by | Rate |
|---|---|---|---|
| C1 | node | mining at least one **accepted block** in the epoch window, credited to the block's payout address | **200 points per epoch**, flat — mining more blocks does **not** raise it |
| C3 | verify | **accepted transactions** sent in the epoch window, credited to the address | **1 point per 100 transactions** (rounded down, and capped per epoch) |
| C2 | bug | a reported bug, priced by severity (S0 5000 / S1 2000 / S2 500 / S3 100); a duplicate is 10 % of a first report | operator award after review |
| C4 | infra | infrastructure contribution | operator award after review |
| C5 | LLM replica | accepted, k=2-matched PALW replica work — 1 point per accepted slot, flat | collection is implemented, but **not part of the current `testnet-10` hourly job** |

The C1 flat rate is not a rounding of the ledger — it is what the live ledger actually pays. In the
current fact store one address with **391,635** mined blocks and another with **764** both score
**exactly 200** C1 points for the same window. C1 rewards *that you ran a node worth counting*, not
how much hash you pointed at it, so a large miner and a small one are level on C1 and separate on
nothing else automatic.

C3 is the same idea applied to usage: 4,101 accepted transactions scored 41 points; 32 transactions
scored 0. Transactions have to be *accepted on the canonical chain* — activity on a branch that the
chain later reorgs away does not count, by design (`docs/testing/mtp-epoch2-partition-policy.md`).

**C2 and C4 are hand-awarded** by the operator after review, and need no registration because the
operator asserts the attribution directly.

## 2. Taking part

### Step 1 — make an address

This key *is* your identity for points. Back it up; it cannot be recovered.

```bash
cargo build --release --bin misaka
./target/release/misaka key gen --network testnet-10 --out mtp.seed
```

It prints your `misakatest:…` address. That address is your ledger id — it appears on the leaderboard
as `addr:misakatest:…`. **Nothing else is required to start earning C1/C3.** Since 2026-08-02 any
well-formed address scores for what it did on chain; the older invitation/registration handshake is
only needed if you want your points to accrue to a GitHub handle (`gh:<you>`) instead.

### Step 2 — run a node on testnet-10

```bash
cargo build --release --bin kaspad
./target/release/kaspad --testnet --netsuffix=10 --utxoindex \
  --addpeer=160.16.131.119:26211
```

`misaka join --network testnet-10` is a friendlier front-end that names the DNS seeds for you. Check
you actually joined and are in sync before expecting anything to score — a node still in IBD is
reachable but not usable, and it earns nothing:

```bash
./target/release/misaka node doctor --network testnet-10
```

### Step 3 — mine to your address (C1), and use the chain (C3)

```bash
cargo build --release --bin kaspa-pq-miner
./target/release/kaspa-pq-miner --network-id testnet-10 --rpc 127.0.0.1:26610 \
  --pay-address misakatest:<your-address> --blocks 0 --min-block-interval-ms 1000
```

One accepted block in the window is enough for the full 200 C1 points. Sending transactions from the
address (`misaka wallet send …`) accrues C3 at 1 point per 100 accepted transactions.

### Optional — a GitHub-handle identity

If you would rather have points accrue to `gh:<your-handle>` (and to receive C2/C4 awards under it),
open an issue on this repository with your handle and your address, sign the invitation you get back
offline, and submit the result:

```bash
./target/release/misaka mtp register --network testnet-10 \
  --invitation invitation.json --key-file mtp.seed --out registration.json
```

The MTP HTTP surface is **read-only by design** — there is no registration endpoint, and therefore
none that could accept a forged registration. One handle binds to exactly one address, and nothing
before registration is retroactive.

## 3. Checking your points

The public, read-only query API — no account, no login:

```
https://misakascan.com/mtp/v1/...
```

| Route | Returns |
|---|---|
| `/mtp/v1/points` | the full leaderboard, every id ranked |
| `/mtp/v1/points/<id>` | one identity, e.g. `addr:misakatest:qtp…` or `gh:alice` |
| `/mtp/v1/epoch/<n>` | the signed ledger for epoch `n` (latest issue) |
| `/mtp/v1/epoch/<n>/facts` | the exact inputs that ledger scored |
| `/mtp/v1/epoch/<n>/all` | every issue of epoch `n`, including superseded ones |
| `/mtp/v1/operator` | the operator's 2592-byte ML-DSA-87 public key and its out-of-band pins |
| `/mtp/v1/rules/<hash>` | the frozen rule set a ledger was scored under |

```bash
curl -s https://misakascan.com/mtp/v1/points                        # leaderboard
curl -s https://misakascan.com/mtp/v1/points/addr:misakatest:<your-address>
```

The operator key is pinned here so you can check the endpoint is not lying about who signs:

```
misakatest:qtu8yq0psff2leaz35rqrh5kcz20kug5jce2ecca9hx7ed6cxpghrzhnjg650ugu7esa8snj2ltz4v0dkdzu0dn7s90xmakw0fneety0pvngw4r0
```

`/mtp/v1/operator` must surface that same string in its `pins`. If it does not, do not trust the
ledgers it serves.

**The bundled CLI cannot reach the HTTPS endpoint.** `misaka mtp points` / `misaka mtp leaderboard`
speak plain HTTP/1.1 with no TLS, so they work only against an `http://` instance — a local service
or a tunnel to one:

```bash
misaka mtp points addr:misakatest:<your-address> --endpoint http://127.0.0.1:8790
misaka mtp leaderboard --endpoint http://127.0.0.1:8790
```

That is a client limitation, not a hole in the trust model. Fetch over HTTPS with `curl`, then verify
locally — which is the part that actually proves something.

## 4. Verifying a ledger yourself

```bash
curl -s https://misakascan.com/mtp/v1/operator \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["operator_pubkey_mldsa87_hex"])' > operator.pub

curl -s https://misakascan.com/mtp/v1/epoch/1        > epoch-1.jsonl
curl -s https://misakascan.com/mtp/v1/epoch/1/facts  > epoch-1.input.json

misaka mtp verify-epoch epoch-1.jsonl --pubkey-file operator.pub
misaka mtp verify-epoch epoch-1.jsonl --pubkey-file operator.pub --facts epoch-1.input.json
```

Without `--facts` it checks the ML-DSA-87 signature and the rules hash. With `--facts` it *re-runs the
scoring deterministically* and byte-compares the result against the signed ledger. All arithmetic in
the scorer is integer (points are carried as milli-points — a ledger's `"c1": 200000` is 200 points),
so the recompute is bit-reproducible on any platform. A ledger that passes both did not come from
trusting the server.

Every fact carries its evidence: C1 rows cite the block hashes, C3 rows the transaction ids. If your
points look wrong, `/mtp/v1/epoch/<n>/facts` shows exactly what was counted for you.

## 5. Epochs — when points start counting

An epoch is a weekly window `[Monday 00:00:00Z, +7 days)` that the operator publishes over. Two
consequences worth being explicit about:

- **Nothing accrues before the first published epoch, and the service does not backfill.** Facts are
  collected continuously, but they become points only when an epoch that covers them is published.
- **Publication is an explicit operator command, never a cron.** A signed ledger is an artifact
  participants are entitled to rely on, so it is not something a timer emits unattended.

Corrections are the designed path, not an exception: a reissue is a new, fully-signed
`epoch-<n>.<issue>.jsonl`, old issues are never deleted, and `index.json` records the supersede
ordering. An epoch becomes immutable only once the finality horizon passes it.

**Published so far**

| Epoch | Window (UTC) | State |
|---|---|---|
| 1 | 2026-08-07 00:00 → 14:29 | published, issue 0, `finalized: false` |
| — | 2026-08-08 → 2026-08-15 | **skipped, unscored** — the network was partitioned, halted and flag-dayed that week; grading participants on the operator's outage would be wrong (`docs/testing/mtp-epoch2-partition-policy.md`) |
| 2 | 2026-08-15 00:00 → 2026-08-22 00:00 | scheduled |

Where a scoring window touches the 2026-08 fork, **only the canonical lineage counts**. Operator,
premine and fleet addresses score under the same rules as everyone else and are labeled as
operator-run rather than hidden — the leaderboard's top row today is the operator's cold-start miner.

## 6. Current status

**As of 2026-08-15, collection is paused and the query API is down for maintenance.** Being able to
see this is the point of publishing it:

- The last chain fact ingested is from **2026-08-12 01:28Z**. From 08-12 06:00Z the hourly scan found
  no new blocks (the chain was stalled), and from 08-14 17:00Z it fails outright because the explorer
  database is stopped.
- `misaka-mtp.service` was stopped 2026-08-15 01:54 JST along with the rest of the explorer stack, so
  `https://misakascan.com/mtp/v1/...` currently answers **502**.
- Epoch 1 remains the only published ledger. No points have been lost: facts are re-ingestible from
  the chain, and the collector deduplicates on `(kind, evidence, address)`, so a wide re-scan after
  the outage recovers the window rather than double-counting it.

## 7. FAQ

**Do I have to register?** No, not for C1/C3. Any well-formed `misakatest:` address scores for its
own on-chain work. Register only if you want a `gh:<handle>` identity.

**I mined a lot of blocks — why only 200 points?** C1 is flat per epoch by design. Mining more does
not raise it; the extra hash is not the contribution being measured.

**My transactions did not score.** C3 is 1 point per **100** accepted transactions, rounded down — 99
transactions score 0. And the transactions have to be accepted on the canonical chain.

**My activity is in the 2026-08-08 → 08-15 week.** That week is unscored for everyone, including the
operator. It is not a penalty aimed at anyone; the network was split for it.

**Points did not appear right after I mined.** Points appear when the epoch covering them is
published, not in real time. Facts are collected hourly; ledgers are published per epoch.

**Are the numbers on the site authoritative?** The *ledgers* are. The site mirrors them. Verify with
`misaka mtp verify-epoch --facts` and believe the result, not the page.

---

**See also** — [`docs/testing/mtp-epoch2-partition-policy.md`](testing/mtp-epoch2-partition-policy.md)
(the operator decisions covering the 2026-08 partition), [`docs/validator-runbook.md`](validator-runbook.md)
(running a validator).
