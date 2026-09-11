# Mining with the A16 model on testnet-11 — from `v3 executed` to a paid block

This page is for an operator who already has the free-prompt gateway answering prompts — the
worker prints `[palw-worker] [qwen25-a16] v3 executed: prefill=… decode=… exec root=…` — and
cannot tell whether any of it is mining. **On its own, it is not.** An executed job is an answer
and a commitment waiting in a directory. It becomes mining only when four more things happen,
and three of them need processes the gateway does not start:

| # | what has to happen | who does it | how you see it |
|---|---|---|---|
| 1 | the job's commitment is signed, funded and submitted (a `0x4a` carrier transaction) | **`misaka-palw-fp-rail --watch`** — not the gateway, which holds no key | the watcher logs `SUBMITTED carrier <txid>` then `claim <id> is on the chain` |
| 2 | a panel of 5 seats replays checkpoint intervals of the job and files `Valid` | other operators' seats — but they fetch each interval **from your node** | `misaka palw claim <id>`: `panel_bound` → `receipt_licensed` |
| 3 | the claim survives its challenge window and becomes `Final` | the chain (1,200 DAA after licensing) | `misaka palw claim <id>`: `final` |
| 4 | each winning quantum of the claim is spent as a **receipt block** (PoW algo 7) | **your own `kaspad --palw-produce`, running the same bond and key** | `[palw-producer] produced RECEIPT block #N …`, then a coinbase output at your pay address |

The reward is step 4, and only step 4. A free-prompt claim carries no escrow and pays nothing at
`Final`; `Final` makes its quanta drawable, and every quantum that wins its draw licenses one
block that your producer mines and is paid for like any other block. On testnet-11 that is
**275,628,448,680 sompi (≈2,756 MSK) per receipt block** today (measured at the pay address of
the claim below). Nobody else can spend your quanta: a receipt block that names another bond's
claim is refused (`ProducerNotExecutor`).

**A worked example, from the live chain.** Claim `019efe78…` (a 300-token answer, pool slot-05):
accepted at DAA 1305 on 2026-09-05 06:23Z, licensed by three seats at 16:59Z the same day, final at
DAA 2729, then on 2026-09-10 at 09:01Z the slot's own producer logged 25 lines of
`produced RECEIPT block #N … (a certified free-prompt claim, mined)` — 25 of its 51 quanta had won
their draw — and the slot's pay address received the matching ≈2,756 MSK coinbase outputs
(DAA 3132 onward). `misaka palw claim 019efe78…` shows it today as `phase final at DAA 2729`
and `quanta 25 of 51 spent`, with the draw slot (DAA 3129) and what can still happen to the rest.

Timing, stated as it is rather than as the design says: every window is a DAA count (bind 600,
receipt 600, challenge 1,200, draw maturity 400, use window 600 — the shipped
`PALW_RC_WINDOWS_V1`), and testnet-11 has been producing about **13 DAA per hour** (measured
2026-09-10/11), not the 30 of its frozen 120 s cadence. So expect a panel within about 2 hours,
licensing within hours if your node serves the seats, `Final` about four days later, and the draw
about a day and a half after that. **Your producer must be running for the whole use window (600
DAA, about two days) after the draw**: a winning quantum not spent inside it is gone.

---

## 1. What you need beyond the gateway

| | |
|---|---|
| a `kaspad` from `main` | on the network's current ruleset — its first log lines print the fingerprint and `Consensus fence schedule: 1150, 1900, 2150, 2400, 2125000`; see [testnet11-join-mining.md](testnet11-join-mining.md) §1 |
| a registered bond | [testnet11-join-mining.md](testnet11-join-mining.md) §2–§3; the floor-sized default holds two or three 256-token claims at once (§3 below) |
| the tokenizer-**bound** A16 artifact | the same file for the gateway's worker AND for `kaspad --palw-class-artifact` (§2) |
| a second output at the bond key's address | ≥ 0.1 MSK, for the carriers' fees — the registration carrier's change is reserved by the node's panel and cannot be used (§4) |
| `--utxoindex` on the node | so the watcher (and `misaka wallet`) can find that output |

## 2. The artifact: bound, and the same file everywhere

The worker refuses an artifact whose `tokenizer_commitment` is all zeros, and the published
`qwen25-1.5b-a16.palwart` (sha256 `a8c4e53e…`) is one. Bind it once:

```bash
cargo build --release -p misaka-palw-sdk --bin palw-class
./target/release/palw-class bind-tokenizer --network testnet-11 --tokenizer /abs/tokenizer.json \
  --out /abs/qwen25-1.5b-a16.bound.palwart --model-id 'Qwen/Qwen2.5-1.5B/graph-v5@512' /abs/qwen25-1.5b-a16.palwart
shasum -a 256 /abs/qwen25-1.5b-a16.bound.palwart    # 3f8fc5066bafae28… — the file the testnet-11 fleet runs
```

It resolves to class **`4277d84f…`** (`Qwen/Qwen2.5-1.5B/graph-v5@512`), the one A16 class that
is certified on the free-prompt lane today — check any id with `misaka palw certified <128 hex>`.
Its width is **512 tokens, prompt and answer together**; the gateway's default answer length is
256.

**Give your node the same file, and only that one**, with `--palw-class-artifact=/abs/qwen25-1.5b-a16.bound.palwart`.
Your node is the only place a seat can get openings of your claim from (the capture is far over
the 16 MiB transport cap), and it computes them with this artifact. The node keeps the FIRST
artifact of a family it is given: listing the unbound file too makes every opening it serves
disagree with your claim's output root, and the seats refuse an honest answer.

## 3. How big a bond, per claim

A claim reserves `pwu × slash` on your bond until it is `Final`, and a bond may back at most half
its collateral. For class `4277d84f…` (`pwu_per_inference` 6,630,544, slash 5, 8 quanta per
canonical job, at most 64 per claim):

| answer | leaves (measured) | quanta | reserved exposure | collateral per claim in flight |
|---|---|---|---|---|
| ~40 tokens | 6.6 M | 8 | 33,152,720 sompi | 0.66 MSK |
| 256 tokens (the default) | 33.2–34.4 M | 40–41 | 165,763,600–169,907,690 | 3.4 MSK |
| 300 tokens | 42.3 M | 51 | 211,348,590 | 4.2 MSK |
| the cap | ≥ 53.0 M | 64 | 265,221,760 | 5.3 MSK |

The default floor bond (1,110,106,160 sompi of collateral, 555,053,080 of ceiling) therefore
holds two or three default-length claims at once. The gateway checks each answer's own exposure
against the bond's room before it writes a commitment, and the watcher checks again before it
spends a fee; a claim that does not fit waits until earlier claims reach `Final`.

This is the check that was missing, and what it cost is on the chain. Between 2026-09-04 and
2026-09-09 the fleet's own pool slots submitted 33 free-prompt carriers, and five became claims.
Seventeen of the rest were mined and accepted with their fees paid, and the chain dropped the
commitment inside each one. The slot node's log gives the reason, for example `bond … backs
161658050 and this claim would reserve 186484050, above its exposure ceiling 265221760 (admission
item 8, free-prompt lane)`. The other eleven never reached an accepted block. One slot's bond had
room for ONE default-length claim, the gateway had checked the room against a canonical claim
(one fifth of the real figure), and its submitter sent every job it had. (Counted 2026-09-11 with
`getPalwFreePromptClaim` for each claim and the explorer's transaction record for each carrier.)

## 4. Run it: node, gateway, watcher

**The node** — the producer (it mints your receipt blocks, and floor blocks meanwhile), the panel
(it serves your claims' openings to their seats) and the artifact:

```bash
kaspad --testnet --netsuffix=11 --appdir=$HOME/.t11 --utxoindex \
  --listen=0.0.0.0:26311 --rpclisten=127.0.0.1:26312 --rpclisten-borsh=default \
  --addpeer=169.58.39.220:26311 \
  --palw-produce --palw-panel \
  --palw-producer-key=$HOME/.misaka/miner.seed \
  --palw-producer-bond=<bond txid>:0 \
  --palw-fee-outpoint=<bond txid>:1 \
  --palw-class-artifact=/abs/qwen25-1.5b-a16.bound.palwart
```

(No `--palw-producer-class`: the producer mines the floor, which needs no bond beyond the default
and is never out of epoch budget. The receipt lane is the same producer loop either way.)

**Fund the carriers.** The node reserves the registration carrier's change (`<bond txid>:1`) for
its panel, so give the bond key's address a second output and leave it to the watcher. (If your
only funds are the faucet's 12 tMSK, split them BEFORE registering the bond — `misaka wallet send
--to <your address> --amount 0.5 --key-file <seed> --yes` — so the registration takes the larger
output and the 0.5 MSK stays free for the watcher.)

```bash
misaka --network testnet-11 key address --key-file $HOME/.misaka/miner.seed
# send ≥ 0.1 MSK there from any other wallet; one output funds hundreds of claims (each carrier
# pays a fee of ~0.003 MSK and returns the rest as change the next carrier spends)
```

**The gateway's identity**, from the node rather than by hand:

```bash
misaka-palw-fp-rail --print-identity --bond-key-seed $HOME/.misaka/miner.seed \
  --rpc 127.0.0.1:27210 --class-id 4277d84f7d91528cc04aa366d51ee1c2e4f7902c4f6b16a213dead1c7e227977db732f18ed6183db3d944d44726ebd3feff7b15c48f9dba11cd526684f35f1b7 \
  > $HOME/.misaka/fp-identity/identity.json
```

It finds the bond registered to that key (or takes `--bond <txid:index>`), reads its operator id,
derives the network domain from the node's genesis, and warns if the class is not certified on the
free-prompt lane. **Keep the seed out of `identity.json`'s directory and out of the outbox** — the
gateway refuses to start if it can reach a signing secret there.

**The gateway** (as you run it now; every path absolute):

```bash
MISAKA_PALW_NETWORK_ID=testnet-11 \
MISAKA_PALW_ARTIFACT=/abs/qwen25-1.5b-a16.bound.palwart \
MISAKA_PALW_TOKENIZER=/abs/tokenizer.json \
misaka-palw-gateway --listen 127.0.0.1:8790 \
  --worker /abs/palw-a16-fp-worker \
  --outbox $HOME/.misaka/fp-outbox \
  --identity $HOME/.misaka/fp-identity/identity.json \
  --rpc 127.0.0.1:27210
```

Its boot line says which class it serves and at what width
(`listening on … class 4277d84f…, n_ctx 512`), and the next line what the chain says about it:
`registered true | fp_certified true | bond_active … | exposure_room …`. `curl -s
127.0.0.1:8790/health` repeats both. For committing, the fields that matter are `registered`,
`fp_certified`, `bond_known` and `exposure_room`. `bond_active` is the attempt lane's readiness
(false while the class's attempt budget is spent, for example), and it is not a condition of
committing.

**The watcher** — the process that carries every committed job to the chain, one at a time:

```bash
misaka-palw-fp-rail --watch $HOME/.misaka/fp-outbox \
  --bond-key-seed $HOME/.misaka/miner.seed --rpc 127.0.0.1:27210
```

Run it on the node's host (it stages each claim's capture into the node's own retention
directory, which the node names over RPC), under systemd or tmux. It finds its first funding
itself on a node with `--utxoindex` (or takes `--funding-outpoint <txid:index> --funding-amount
<sompi>`), then funds each carrier from the previous one's change. It never submits a job whose
claim would not fit the bond, and it does not start the next job until the previous carrier has
become a claim, or has been reported dropped. It keeps its state in
`<outbox>/rail-watch-state.json`. `--once` runs one pass, for cron.

## 5. What a healthy run prints

For one prompt, in order (DAA figures at today's cadence):

```
# gateway / worker (seconds to minutes)
[palw-worker] [qwen25-a16] v3 executed: prefill=… decode=256/256 in …ms (… leaves); exec root=…
[misaka-palw-gateway] fp-job-<16 hex>: committed claim <claim> (40 quanta, 165763600 sompi of exposure) — in the outbox, NOT on chain until misaka-palw-fp-rail submits it (--watch …)
    (or: `answered, not committed — <reason>`; the HTTP answer's `misaka` object says the same)

# watcher (within one --interval)
… [misaka-palw-fp-rail] fp-job-<16 hex>: submitting claim <claim> (funded by …, the lane's funding selector)
… [misaka-palw-fp-rail] fp-job-<16 hex>: SUBMITTED carrier <txid> — pwu 33152720 quanta 40 fee … sompi
… [misaka-palw-fp-rail] fp-job-<16 hex>: carrier <txid> is in the mempool — the next job waits for it to become a claim
… [misaka-palw-fp-rail] fp-job-<16 hex>: claim <claim> is on the chain — phase provisional, accepted at DAA …

# your node, while seats verify (the next hours) — every node logs the lifecycle a block carries
Block <hash>: PALW lifecycle carried 1× PanelBound
[palw-panel] claim <claim>: opened interval 57 of the retained capture (1110705 bytes served, 19s)
    (one line per interval a seat drew — typically four per seat; this is your node answering them)
Block <hash>: PALW lifecycle carried 1× ReceiptLicensed

# days later: the draw, and the pay
[palw-producer] produced RECEIPT block #1 <hash> (a certified free-prompt claim, mined)
```

At any point, ask the chain rather than the logs:

```bash
misaka --network testnet-11 palw claim <claim id>          # phase, quanta, quanta_spent, what happens next
misaka --network testnet-11 palw claim --outbox $HOME/.misaka/fp-outbox   # every job in the outbox, how far it got, and a tally
misaka --network testnet-11 wallet utxo list --key-file $HOME/.misaka/miner.seed   # the coinbase outputs, once paid
```

A paid receipt block appears as a coinbase output at the producer's pay address. It is spendable
after the chain's coinbase settlement rule (a DNS anchor past it, or 600 DAA).

## 6. When it does not

| you see | it means | do |
|---|---|---|
| `v3 executed` and nothing on chain | nothing is submitting: the gateway never does. The worker prints `v3 executed` for every job, committed or not; the gateway's own line after it says which | run the watcher (§4) |
| `"committed": false` in the answer | `not_committed_because` names the reason, in the chain's words: unknown class, uncertified lane, unknown bond, no room for this answer's exposure, or the window budget | fix what it names. The answer is still delivered, just not mined |
| watcher: `holding — its claim would reserve … sompi and the bond already backs …` | the bond is full until earlier claims reach `Final` | wait, or register a larger bond from a new key |
| watcher: `WARNING … the chain holds no claim …` | the carrier landed and the chain dropped the commitment inside it — most often the bond's ceiling: the node log has `a PALW lifecycle object was dropped, and the block stands: bond … backs N and this claim would reserve M, above its exposure ceiling C (admission item 8, free-prompt lane)` | the fee is spent. The watcher's exposure check exists to stop this happening again |
| watcher: `no funding: …` | no spendable output at the bond key's address (or no `--utxoindex`) | send it ≥ 0.1 MSK, or pass `--funding-outpoint` |
| `misaka palw claim`: `voided (receipt_timeout)` | the seats never got enough of your claim to file `Valid`: most often your node runs no `--palw-panel`, has no (or the wrong) `--palw-class-artifact`, was offline, or the capture was not staged | fix the node, then ask again (a void costs no bond on testnet-11, only the claim) |
| `[palw-interval] refused … : not-bonded` for claims that are not yours | pre-fix builds printed this on every node without a panel, for every seat request on the network. Seats ask every peer. It is not about your bond | nothing. Current builds print one `not-serving` line and stay quiet |
| the same for **your** claim | your node runs no panel, so it serves nobody | add `--palw-panel` and the bound artifact |
| `a PALW lifecycle object was dropped, and the block stands: claim …'s receipt set does not carry a quorum` | someone's `ReceiptLicensed` carrier arrived before its quorum, or after the claim had already moved on (`WrongPhase`). The block stands, and so does the claim | nothing, even when the claim is yours: licensing happens when three `Valid` receipts ride together |
| the worker refuses: `prompt N + decode ceiling M exceeds max_context_tokens 512` | prompt plus `max_tokens` is over the class's width | ask for fewer tokens |
| the worker refuses at boot: `this artifact declares no tokenizer` | the unbound artifact | §2 |

**Stopping.** A claim's openings exist only on your node until it is licensed, and a challenge
before `Final` needs them again. Keep the node, its panel and its `palw-retention/` directory for
as long as `misaka palw claim --outbox` shows a claim that is not yet `final` or `voided`. A
producer keeps a live free-prompt claim's capture past its usual 48-hour prune for exactly this
reason.
