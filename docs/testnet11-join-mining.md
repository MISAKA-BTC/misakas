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

`--netsuffix=11`, P2P **26311**, gRPC **26312**, **wRPC-borsh 27210**. DNS seeding is live, so no
`--addpeer` is needed

**Two RPC ports, and the tools on this page use the second one.** `--rpclisten` sets the gRPC
port; `misaka`, `misaka-palw-gateway` and `misaka-palw-fp-rail` all speak **wRPC-borsh** and their
`--rpc` flag wants that port. A node booted with `--rpclisten` alone logs
`node-wrpc-borsh: disabled`, and every tool below then fails with
`invalid HTTP version (node up with --rpclisten-borsh?)`. Pass `--rpclisten-borsh=default` as well
— it resolves to 27210 on testnet-11 — or omit `--rpc` entirely and let `--network testnet-11`
supply the default
(fallback entry nodes: `169.58.232.113:26311`, `169.58.39.220:26311` — the two the seeders
verify and advertise. `5.104.81.23` does not accept inbound connections and `169.58.232.114`
was withdrawn on 2026-08-29.)
A node on the right chain logs

```
Consensus params fingerprint: a7baab7957d27bbd2591cd24f70ee92b555ab26cd49ef425cbd7093f06e222d9 (network testnet-11)
```

(Identity as of **Relaunch 5e, 2026-09-02** — genesis `08e9c8a4…` (`PALW_RC_GENESIS`), which 5e
keeps. **Do not read that as a property of the chain.** Genesis held from 5c through 5e because
those relaunches moved only the ruleset over it; a relaunch that changes the genesis UTXO set —
premine entries, community allocations, the class set seated at block zero — moves the genesis
hash itself, and then no earlier appdir and no earlier binary can join at all. Both values below
are dated facts about a past relaunch, not constants: take the live ones from your own node's
first log lines rather than from this page. Earlier values —
`e2b91c16…` (5d), `d38abe44…` (5c), `f0e50f83…` and `accaadce…` (all 2026-09-02), `5ccdd684…`
(2026-08-31), `f3bf86b4…` (2026-08-30), `95265934…` (2026-08-29) — name archived rulesets; a node
still announcing any of them is refused at the handshake by every node on this one. **Wipe the
appdir whatever you joined before**: the fleet archives its datadirs at every relaunch and starts
an empty chain, and 5e's state version refuses an older one at boot outright.)

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

The easiest way to get that first transfer is the public faucet on the explorer:
<https://misakascan.com/#/faucet> pays 0.5 tMSK per address (once, ever) as a regular
transaction — exactly the non-coinbase output the bond path needs, and over a hundred times
the 0.004 MSK collateral floor.

**Fund it with a normal transfer, not with mining rewards.** Two separate rules bite a coinbase
output: `coinbase_maturity`, and the ADR-0018 DNS settlement floor
(`coinbase_settlement_long_maturity_daa` = 600 on testnet-11). On top of that the node's funding
scan skips coinbase entries outright, so a coinbase UTXO will not be found at all and the only
symptom is "no confirmed UTXO to spend".

---

## 3. Register a bond

```bash
kaspad --testnet --netsuffix=11 --appdir=~/.t11 \
  --listen=0.0.0.0:26311 --rpclisten=127.0.0.1:26312 --rpclisten-borsh=default \
  --addpeer=169.58.39.220:26311 \
  --palw-register-bond \
  --palw-producer-key=~/.misaka/miner.seed \
  --palw-producer-pay-address=<your misakatest: address>
```

The node waits until it is synced, builds one `BondRegistered`, submits it in a transaction that
**locks the collateral in its own output**, and then waits for the bond to actually appear on the
chain before telling you it did:

```
[palw-panel] registered bond <txid>:0 with <n> sompi of collateral, in tx <txid>.
Restart with --palw-producer-bond=<txid>:0 (and --palw-produce) to mine with it
```

**That line is the only place the bond's outpoint appears.** It is this transaction's own id, which
did not exist until the transaction was built — nobody can tell it to you in advance, and the node
does not store it anywhere else. Keep it.

If instead you see

```
[palw-panel] carrier <txid> was accepted but no bond appeared within 10 minutes.
```

then the transaction landed and no bond was created. **Your collateral is not lost** — the output
is yours and spendable; only the fee is gone. Mempool admission for a lifecycle carrier sees just
the payload (decode, wire version, may-ride table), so it cannot check the carrier binding, which
means a network whose nodes predate the index-and-zero-id naming accepts the transaction and then
drops the registration on extraction. Check that the network runs a build that accepts this form.

If it cannot proceed it says why, once per reason rather than every five seconds. The usual reason
is that no confirmed non-coinbase UTXO is visible yet; fund the address and it picks it up without a
restart.

**There is no `--palw-fee-outpoint` in that command, and there should not be.** The node finds its
own money by reading the UTXO set for outputs under the address it is about to name as payee — a
newcomer has no outpoint to be told, since the only one it will ever have is the change of the
carrier it has not built yet. The flag is for a seat that already has an outpoint to spend — a
genesis fee float, or its own carrier's change — and §4 is where it matters.

That is worth stating because it used to be false in a way nothing revealed: the funding resolver
returned early when the flag was absent, skipping the scan entirely, and reported

```
[palw-panel] cannot register a bond yet: no confirmed UTXO to spend — send at least 400000 sompi
plus a fee to this node's pay address
```

against an address that `misaka wallet utxo list` showed holding 10 MSK, mature, on the same node's
RPC. If you are running a build from before 2026-08-26 and see that line while the address is
funded, pass `--palw-fee-outpoint=<funding txid>:<index>` to work around it.

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

**The relay limit sets a second, higher floor, and it is the one that bites first.** A UTXO's
KIP-0009 storage mass is `C · p² / value`, so it grows as the output SHRINKS: a 400,000 sompi
output costs 10,000,000 mass against a 480,000 standard-transaction limit, and the carrier holding
it is refused as non-standard no matter how it is funded —

```
the carrier was refused: transaction ... is not standard: transaction storage mass of 10000003
is larger than max allowed size of 480000
```

On testnet-11 that puts the smallest carryable collateral at **8,333,924 sompi** when the funding
UTXO holds 10 MSK — twenty times the chain's own floor. (The exact number moves with the funding
amount and the fee, because the change output pays mass too.) The node computes it from the funding
UTXO it is about to spend and **raises its default to fit**, saying so:

```
[palw-panel] raising collateral from 400000 to <n> sompi — an output of 400000 costs 10000003
storage mass against a relay limit of 480000, so a carrier holding one cannot be submitted.
Pass --palw-bond-collateral to choose the amount yourself.
```

A collateral you named yourself is **not** raised — it is your money and your exposure ceiling, so
too small a value is refused with the number that would work instead. If the funding UTXO is small
enough that no split of it clears the limit, the message says that too: send more, rather than
reaching for the collateral knob.

---

## 4. Produce

```bash
kaspad --testnet --netsuffix=11 --appdir=~/.t11 \
  --listen=0.0.0.0:26311 --rpclisten=127.0.0.1:26312 --rpclisten-borsh=default \
  --addpeer=169.58.39.220:26311 \
  --palw-produce --palw-panel \
  --palw-producer-key=~/.misaka/miner.seed \
  --palw-producer-bond=<txid>:0 \
  --palw-producer-pay-address=<your misakatest: address>
```

All **five** of key, bond, pay address, a class and a fee outpoint are required or the producer does
not start at all.

**`--palw-fee-outpoint` is mandatory here, and this paragraph used to say it was a choice.** The
command above PANICS at startup without it:

```
panicked at kaspad/src/daemon.rs: --palw-produce on a ConsensusV2 network needs a way to carry
lifecycle objects: pass --palw-fee-outpoint <txid>:<index>
```

The receipts-only mode described below is the **panel seat's** rule, not the producer's: the gate
that panics keys on `--palw-produce`, and a seat that does not produce never reaches it. The only
node that starts without the flag is one whose previous run persisted
`<appdir>/misaka-testnet-11/palw-panel/palw-fee-outpoint` — which a first run does not have. So for
a panel seat, and only for a panel seat, this is still true:

```
[palw-panel] starting (bond=…, submitter=off — receipts only)
```

It will answer and file, but it will not carry anything to the chain. Pass
`--palw-fee-outpoint=<txid>:1` — the change output of your own bond carrier — to turn the submitter
on. (Registration is the exception: that job has no outpoint to be given and finds its own funding.)

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
every node. **No GGUF, no download, no worker binary.** The floor is also exempt from the per-class epoch
budget, so it is the one class that can always produce.

To mine a registered model class instead, pass its id with `--palw-producer-class` and give the node
that class's converted artifact (`--palw-class-artifact`). A class registered while an epoch is
already running has share from the moment it activates. Since ADR-0053 there is one execution
family, so any class you can be pointed at is one this build can execute and the court can
adjudicate — there is no second verification scheme to be on the wrong side of.

---

## 6. What a bond costs you

The collateral is locked in the output the registration names and is reclaimable at your pay
address once the bond is retired (an owner ML-DSA-87 signature over the bond key releases it). It is
also what a court can slash if this node commits a provably wrong execution — that is the whole
point of it, and it is why the exposure ceiling exists.

---

## 6b. Do not stop your node with claims in flight

Every block you produce opens a **claim** that lives on chain for hours (bind → receipts →
challenge → court; the whole lattice is several thousand DAA). Until it resolves, **your node is
the party responsible for serving that claim's execution material** — the panel seats verify what
you produced from the bytes you broadcast, and a claim whose material nobody can obtain is
**voided and slashed against your bond**. That is the data-availability half of the protocol, not
a bug: work you cannot show is work nobody can check.

Practical rules:

* mine only while you can leave the node up for the day — if you must stop, expect the claims
  from your last few hours to default and cost `pwu × slash_value_per_pwu` each off your bond;
* the fleet also remembers: since protocol 104 every panel seat persists any material it has
  heard and **re-serves it on request** (`PalwMaterialRequest`), so a brief restart is survivable
  as long as your material reached at least one live seat while you were up. A node that was
  never well-connected has no such safety net — check your peer count before relying on it;
* your retention directory (`palw-retention/` under the app dir) is the durable copy the node
  itself re-serves after a restart. Do not delete it while claims are unresolved.

On 2026-08-28 five outside floor producers mined for a few hours, stopped their nodes, and every
in-flight claim of theirs defaulted with the stake slashed — this section and the pull transport
exist so the next operator does not repeat that.

---

## 6c. Slow classes count too (ADR-0058), and how to mine the LLM classes

The floor produces a block roughly every two minutes; an LLM-class inference takes minutes on its
own. Before ADR-0058 that meant an LLM block almost never won tip selection, and only chain blocks
created claims — so the work went uncounted and unpaid. Since the 2026-08-27 re-mint **the whole
mergeset carries claims**: a red block (which, at `ghostdag_k = 1`, is every block slower than the
floor's cadence) is admitted against the accepting chain state, creates a claim, is
panel-verified, **is paid its worker share to its own miner script**, and moves its class's
per-class difficulty and ADR-0054 share growth. You do not need to win the tip race; you need the
work to be real, because the panel re-derives it and a false claim is slashed against your bond.

To produce in an LLM class instead of the floor, everything in §1–§4 stays the same (same bond,
same key, same node) plus the class artifact and two flags:

| class | artifact | obtain |
|---|---|---|
| `QWEN36` (hybrid 35B, 200‰ share) | `qwen36.palwq36`, 34 GiB, SHA-256 `7a944595a4256ab0…` | [download](https://huggingface.co/Misakachain/Qwen3.6-35B-A3B-PALW-runtime/resolve/main/qwen36.palwq36) or convert from the [source GGUF](https://huggingface.co/Misakachain/Qwen3.6-35B-A3B-PALW-runtime/resolve/main/Qwen3.6-abliterated-35b-Claude-4.7-Q4_K_M.gguf) |
| `QWEN25-A16` (dense 1.5B, 200‰ share) | `.palwart`, 1.7 GiB | convert locally from Qwen2.5-1.5B-Instruct |

Model repository: **<https://huggingface.co/Misakachain/Qwen3.6-35B-A3B-PALW-runtime>**. Verify
before use — the chain pins the artifact **root**, not a filename:

```bash
./target/release/qwen36-run --artifact qwen36.palwq36 --root-only
# must print f4aad4fd543928eb… — anything else is not the registered class
```

Then produce with:

```bash
kaspad --testnet --netsuffix=11 \
  --palw-produce --palw-panel \
  --palw-class-artifact=/path/to/qwen36.palwq36 \
  --palw-producer-class=5bd9ae3d…   # the graph-v3 Qwen3.6 id — take the full value from --palw-dump-classes \
  ... (bond, key, pay-address and fee-outpoint flags exactly as in §4)
```

> **The class ids are the LIVE chain's as of Relaunch 5c (2026-09-02).** `5bd9ae3d…` is the
> corrected `graph-v3` Qwen3.6 registration and `71bbb755…` the dense Qwen2.5-A16 tier; both
> produced accepted blocks on this chain on 2026-09-02. The ids earlier revisions of this page
> named (`ec7bbcbf…`, `f942e268…`) described graphs this build's backend refuses to serve and do
> not exist on this chain. A producer started with an old id points at a class the chain does
> not have.
>
> **Do not copy an id out of any document — including this one.** Ask the binary you are about to
> run, which is the only source that cannot go stale:
>
> ```bash
> kaspad --testnet --netsuffix=11 --appdir=~/.t11 --palw-dump-classes
> ```
>
> **It needs an appdir that has already synced, and on a fresh one it prints nothing at all —
> silently, indefinitely.** The dump waits for a non-zero virtual DAA rather than answering from
> genesis (which would be a confident wrong answer about the tip), and it logs that wait at `trace`
> level. Measured: 45 seconds, zero output, exit only on Ctrl-C. Run it against the appdir your node
> already uses, after it has caught up.

Conversion recipes, per-class hardware requirements, and how a panel seat serves an LLM class are
in [palw-public-testnet-classes-runbook.md](palw-public-testnet-classes-runbook.md). The floor
remains the zero-download path and the liveness guarantee; the LLM classes are where the share
economy (ADR-0054/0056) grows.

**A panel seat needs the artifact of every class it may be seated on.** This paragraph used to say
the opposite, and the measurement it quoted ("a validating seat handed a 33 GiB artifact kept 0.00
GiB of it resident") was real — of a seat that only re-hashed a capture. That is not what a seat
does with a free-prompt claim: it **re-executes the claimed job with the class's own kernels**
(`execute_free_prompt`, `kaspad/src/palw_panel.rs`) and compares roots, and ADR-0077 Decision 8
narrows that to `k` checkpoint intervals rather than removing the replay. A replay needs the
weights. A seat with no backend for a class abstains on it — the panel counts that as
`no backend for the class` — so pass one `--palw-class-artifact` per class you are willing to
verify:

```bash
kaspad --testnet --netsuffix=11 ... --palw-panel \
  --palw-class-artifact=/srv/misaka/qwen36.palwq36 \
  --palw-class-artifact=/srv/misaka/qwen25-1.5b-a16.palwart
```

The floor needs none: its weights derive from a seed on every node.

---

## 7. Mining with your own model — a prompt someone types, mined (ADR-0077)

Everything above mines the **attempt lane**: the node picks the job, runs it, and the block is the
product. This section is the other lane. A person types a prompt, your model answers it, and *that
inference* — the one the person actually received — is the claim. One inference, one commitment:
there is no second, mining-only run, and nothing in the pipeline can create one.

**A block does not follow a prompt.** What follows a prompt is a claim, and a claim walks four
stages before any of its work can be spent. Any interface on this lane says which stage a job is
in, by these names:

| stage | what happened | chain phase |
|---|---|---|
| `submitted` | the `0x4a` commitment transaction was accepted; the claim exists | `Provisional` |
| `bound` | a panel of five seats was drawn for it | `PanelBound` |
| `certified` | the seats replayed it and filed `Valid`; a receipt is licensed | `ReceiptLicensed` |
| `spent` | the receipt matured and one of its quanta paid for a block | `Final`, then spent |

On testnet-11's windows that is **about 80 hours to `Final`, and about 93 hours from commitment to
spendability** — bind, receipt, challenge and then maturity. Fraud-proof safety, not a progress bar
someone forgot to speed up. Show the stage; do not promise a block.

The four windows are DAA-score counts in the shipped bundle — bind 600, receipt 600, challenge
1,200, receipt maturity 400 — and the cadence is the frozen 120 s
(`PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS`). So `Final` is bind + receipt + challenge = 2,400 DAA
= 80 h, and the receipt is spendable at `final_daa + receipt_maturity` = 2,800 DAA = 93.3 h.
**Corrected 2026-09-03:** this paragraph said "roughly 54 hours", which is
bind + receipt + maturity with the 1,200-DAA challenge window left out — the one window the
lifecycle cannot skip, since `palw_producer_v2` states a claim cannot finalize before it has
passed. Read the shipped numbers rather than this sentence:

```bash
cargo test -p kaspa-consensus-core --lib dump_rc_windows -- --ignored --nocapture
# RC windows: bind=600 receipt=600 challenge=1200 court=3000 epoch=1000
#   a claim reaches Final at bind+receipt+challenge = 2400 DAA after acceptance
```

**How many of these the lane can finish in a day is a number, and it is published** (ADR-0082
Decision 12). It is `min(PALW_V2_MAX_PAYOUTS_PER_BLOCK × blocks_per_day, the panel's measured
replay capacity)` — `palw_fp_lane_ceiling_v1` in `palw_economic_locus_v1.rs` computes it and says
which of the two terms is binding. At testnet-11's frozen 120 s cadence the first term is **720
blocks a day × 8 payout rows = 5,760 finalized claims a day**, and that is the ceiling until
somebody measures the fleet's replay rate and finds it lower. Neither half is a knob: the payout
constant is a consensus value whose own doc states the premise it was sized against ("at most one
new claim per block"), and raising it is a ruleset move that owes its own argument. A claim past
the ceiling is not refused — it waits in the payout queue, one more block per eight claims ahead
of it.

**What a claim earns, and what it does not.** Past `Params::palw_fp_decode_rules` (ADR-0082
Decision 10, dormant on every network today) a free-prompt claim's quanta are earned by the leaves
of its **decode calls** — the answer — and the prefill of the prompt is priced at **zero**. The
reason is arithmetic and not policy: the model is deterministic and causal, so every leaf of a
prompt is a pure function of that prompt, and the same bond re-sending a 32,000-token prefix with
one new token recomputes nothing. Paying for prefill would be paying for replay. Who pays the
executor for a long prompt — the requester, in what unit, through what market — is a product
decision outside consensus and this rule does not make it. While the fence is dormant the lane
prices the whole capture, as it does today.

**The fence cannot be armed by this build, and a node refuses to start if you set it.** Neither
half of it exists on the path that would apply it: the state transition has no decode-leaf
enumeration (it answers `FreePromptDecodeLeavesUnavailable`, so an armed chain would refuse *every*
free-prompt claim rather than crediting the answer), and no engine implements
`decode_token_select_v2`, so every temperature job would be refused `SamplingNotArmed` after a full
inference. `Params::validate_palw_v2` therefore refuses a ruleset that arms
`palw_fp_decode_rules` at any height — the fence is a record of a decision, not a switch, until a
build carries both. The same is true of `palw_prompt_ids_merkle` (ADR-0081 Decision 3 / ADR-0082
Decision 5): every writer and every checker in the tree still commits the flat prompt-ids digest,
so arming it would move the network's identity and nothing else.

**And the answer is chosen, not just taken** (ADR-0082 Decision 11, the same fence). Under it the
committed token at each position is a *seeded* argmax — `argmax_j (logit_j × 2²⁴ + T_q × G_j)`
with `G` a Gumbel variate from a pinned table — so `/v1/chat/completions` may carry `temperature`
and `seed` and the answer is a real sample rather than the one repetition greedy decoding produces.
Temperature `0` is the shipped rule byte for byte. **The gateway refuses a temperature or a seed
while its node reports the fence dormant**, by name, before the model is loaded: a job carrying
them would be refused by the transition as `SamplingNotArmed` after you had already paid for the
inference. Since the fence cannot be armed on this build (above), that refusal is the only
behaviour there is today.

### 7.1 The three processes

```
  a browser ──POST /v1/chat/completions──▶ misaka-palw-gateway ──▶ your kaspad (--rpc)
                    SSE tokens ◀──────────         │  spawns ONCE, --mode v3-serve
                                                   ▼
                                        palw-a16-fp-worker  (or palw-qwen36-fp-worker)
                                                   │  the answer, the capture, the four roots
                                                   ▼
                                        <outbox>/fp-job-<id>.*
                                                   │
                                    misaka-palw-fp-rail --submit --rpc  ──▶ the chain
```

* **the gateway** parses the stranger's HTTP, builds the prompt segment-wise, streams the answer,
  and writes the commitment — and **holds no key** (ADR-0079 Decision 4).
* **the worker** is resident: the artifact is mapped once, not once per request. It is the same
  family worker a producer runs, and every job it answers is captured. There is no un-captured
  chat binary left in this tree.
* **the rail** holds the bond key (or asks the signer sidecar for one digest), signs, submits, and
  stages the capture into the node's retention directory — one step, not three.

### 7.2 Run it

You need what §1–§4 already gave you (a registered, Active bond and its key) plus the class's
artifact and tokenizer.

```bash
# 1. the identity the gateway commits under — the class you are serving, and your bond
cat > ~/.misaka/fp-identity.json <<'JSON'
{ "network_domain": "<128 hex — misaka node security-report prints it>",
  "class_id":       "<128 hex — kaspad --palw-dump-classes>",
  "bond_txid":      "<128 hex>", "bond_index": 0,
  "executor_pubkey":"<misaka-palw-fp-rail --bond-key-seed <f> --print-bond-pubkey>",
  "operator_id":    "<128 hex>" }
JSON

# 2. the gateway, on loopback, with the worker under it
MISAKA_PALW_ARTIFACT=/srv/misaka/qwen25-1.5b-a16.palwart \
MISAKA_PALW_TOKENIZER=/srv/misaka/qwen2.5-1.5b/tokenizer.json \
MISAKA_PALW_NETWORK_ID=testnet-11 \
MISAKA_PALW_CONFINEMENT=linux-seccomp-landlock \
./target/release/misaka-palw-gateway \
  --listen 127.0.0.1:8790 \
  --worker $PWD/target/release/palw-a16-fp-worker \   # ABSOLUTE, not ./ — see below
  --outbox ~/.misaka/fp-outbox \
  --identity ~/.misaka/fp-identity.json \
  --rpc 127.0.0.1:27210 \
  --class-leaves <the class's canonical leaves — --palw-dump-classes>

# 3. ask it something — and see §7.2a first: on a 16-token class this is the size that fits
curl -s localhost:8790/v1/chat/completions -H 'content-type: application/json' \
  -d '{"messages":[{"role":"user","content":"the capital of France is"}],
       "max_tokens":3,"stream":true}'

# 4. the one handoff: sign, submit, stage the capture
./target/release/misaka-palw-fp-rail \
  --artifact ~/.misaka/fp-outbox/fp-job-<id> \
  --bond-key-seed ~/.misaka/miner.seed \
  --funding-outpoint <txid>:1 --funding-amount <sompi> \
  --capture ~/.misaka/fp-outbox/traces/<id>/material.bin \
  --retention-dir ~/.t11/palw-retention \
  --submit --rpc 127.0.0.1:27210
```

**`--worker` must be an absolute path.** The gateway confines the worker and pins its working
directory to a scratch directory of its own (ADR-0079), so a relative path is resolved THERE, not
where you typed it. `./target/release/palw-a16-fp-worker` fails with

```
fatal: cannot spawn ./target/release/palw-a16-fp-worker: No such file or directory
```

with the file sitting in the directory you ran from. `$PWD/...` is the fix.

`GET /health` answers the question every operator asks next — *why did my answer not become a
claim* — by name, from the chain rather than from config:

```
"chain": { "registered": true, "fp_certified": true, "bond_active": true, "exposure_room": … }
```

`registered` false means this network does not know your class. `fp_certified` false means the
class is not seated on the free-prompt lane (ADR-0075 `ClassLaneCertified`) and a commitment would
be refused as `FreePromptLaneUncertified`.

**`bond_active` false is usually not about your bond, and this is the field that wastes an
afternoon.** It is computed as "the bond is known AND there is no not-ready reason", and the
not-ready reasons are CLASS-level as well as bond-level. So a bond that is registered, funded and
holding exposure room reports `bond_active: false` whenever its CLASS is out of epoch budget —
which is the normal state of any class registered mid-epoch, for the rest of that epoch. **Read
`bond_not_ready_reason` beside it**; it is in the same `/health` and it names the actual cause:

```
"bond_active": false,
"bond_not_ready_reason": "this class's epoch budget is already spent"
```

Measured on a devnet drill: a class registered at DAA 31 against a 1,000-DAA epoch answered every
request and wrote no commitment for the rest of that epoch. Nothing was broken; the class was
registered mid-epoch. A class seated at genesis has budget from block one. Either way **the user still gets their answer** — the
answer is the product — and the commitment waits in the outbox with the reason attached. A gateway
that silently answered without committing would be lying about what you staked on it.

### 7.2a How wide the answer can be, today

**A class's `n_ctx` is the whole job — prompt and answer together — and it is small.** The worker
serves the width the CLASS registers, read from the catalog row and never from the artifact's own
rotary span, because a runtime answering wider than the court admits would be exactly the
two-products split ADR-0077 R0 closes.

| class registered on testnet-11 | `n_ctx` (prompt + answer) |
|---|---|
| `QWEN25-A16` (dense 1.5B) | **16** — the widest |
| `PALW-BASE-0` (the integer floor) | 12 |
| `QWEN36` (hybrid 35B) | 8 |

(The three genesis rows, as of Relaunch 5e; entrant classes registered later carry their own
width, and `--palw-dump-classes` is the only source that cannot go stale.)

Measured against the shipped Qwen2.5 tokenizer, the ChatML wrapper the gateway must send —
`<|im_start|>user\n … <|im_end|>\n<|im_start|>assistant\n` — is **8 tokens** before your first
word. So on the widest class:

| what you send | prompt tokens | decode tokens left |
|---|---|---|
| `"hello"` | 9 | 7 |
| `"one quiet note"` | 11 | 5 |
| `"the capital of France is"` | 13 | 3 |
| `"Name the second highest mountain in Japan."` | 16 | 0 |
| `"Write a short MIDI melody in C major, four bars, as JSON."` | 23 | **refused — over the class's whole width** |

Over the width, the worker refuses the job and names the numbers rather than trimming:

```
prompt 23 + decode ceiling 256 exceeds max_context_tokens 16
```

That is the current state and it is being worked on: the ladder in ADR-0077 Decision 13 registers
rows at 512, 2,048 and 8,192, and a row's width is inside its class id, so a wider row is a new
class registered beside these rather than a setting on your node. Until one exists, size your
requests from the table above, and do not build a demo that asks for a paragraph.

### 7.3 What a stranger's prompt costs you, and the knobs that bound it

A public prompt becomes **your** claim: it reserves `claim_exposure` on your bond and forfeits it
if your pipeline is faulty. The bound is stated in `/health` under `exposure`, and these are the
flags that set it:

| flag | what it bounds |
|---|---|
| `--claim-exposure-sompi <n>` | what one claim reserves. **`0` (the default) reads it from the chain** — with `--rpc` that is the honest source. |
| `--bond-exposure-room-sompi <n>` | how much room the bond has. `0` reads it from the chain. Without either, the gateway answers and does not commit: a gateway that cannot price the spend does not spend. |
| `--public-job-budget-permille <n>` | the fraction of that room strangers may spend per 24 h, so your own claims are never starved by theirs. |
| `--answer-never-commit` | answer every request, commit none. |
| `--per-source-jobs-per-window <n>` | a courtesy rate limit per source address. Secondary — sources share addresses behind proxies. |

Two bounds are not flags and cannot be raised: one job runs at a time with at most 8 queued, and a
queued commitment **expires with its anchor** (3,000 DAA) and is never submitted stale. The lane's
own ceiling — at most 500‰ of your collateral in flight — is the chain's, printed as
`free_prompt_exposure_ceiling_permille`.

### 7.4 Before you put it on the internet

The default listen address is loopback. Binding a public address is a deliberate act and the
gateway refuses to do it quietly:

* set **`MISAKA_PALW_ALLOW_PUBLIC_GATEWAY=1`** — the acknowledgement that a stranger's text now
  reaches your model host. Without it a non-loopback `--listen` is refused at boot.
* set **`MISAKA_PALW_CONFINEMENT=linux-seccomp-landlock`** so the worker child runs under a real
  platform backend. `/health` prints `confinement_backend`, and it prints **`none`** honestly when
  no backend was installed — a public entrance on a `none` host is the posture ADR-0079 Decision 10
  refuses.
* `--derive-seed` (ADR-0078 signing) must point at a file **outside** `--identity`'s directory and
  outside `--outbox`. The boot check scans exactly those two directories for reachable signing
  secrets and refuses to start if it finds one.
* then ask the host what it actually is, from live state rather than from your config:

```bash
misaka node security-report --worker $PWD/target/release/palw-a16-fp-worker
# exit 0 = OK, 14 = DEGRADED (no backend, nothing public), 13 = EXPOSED (a public
# entrance on a none-backend host, or a public parser holding a key)
```

Worker stderr is **withheld by default** and counted instead: a model runtime line can quote its
input, and "private unless disputed" would be false if the default log were a disclosure. Set
`MISAKA_PALW_GATEWAY_LOG_WORKER_STDERR=1` only when you are debugging your own prompts.

### 7.5 Asking for a thing, not only text (ADR-0078)

The same request can ask for a **derivation**: the model's answer is a DSL, a registered
transformer turns it into an artifact, and what the chain carries is one small record naming both
— never the artifact. (Measured on this build, that record is 3,056 bytes unsigned and about
7.7 KB signed; almost all of it is the ML-DSA-87 key and signature, and none of it grows with the
artifact.)

```bash
curl -s localhost:8790/v1/chat/completions -H 'content-type: application/json' \
  -d '{"messages":[{"role":"user","content":"one quiet note"}],
       "max_tokens":5, "derive":"music/smf/v1"}'
```

> **The width, before you size a request.** `max_tokens` plus the prompt must fit the CLASS's
> registered `n_ctx`, which is the whole budget — prompt and answer together. The widest model
> class this network registers today is **16** (`QWEN25-A16`; the floor is 12, `QWEN36` is 8), and
> the ChatML wrapper is 8 of those before your first word, so the numbers above are what actually
> fits rather than what reads well. Over the width the worker refuses the whole job — `prompt 23 +
> decode ceiling 256 exceeds max_context_tokens 16` — instead of trimming it.
>
> Which means, stated where you meet it rather than discovered later: **a derivation from a real
> inference does not fit on any class registered today.** The shortest MIDI DSL that ships in this
> repository is 118 tokens and the shortest CAD DSL is 76; 16 is the whole job. The transformer
> half works at full size offline, and widening the rows is the work in progress —
> [testnet11-ask-for-a-file.md](testnet11-ask-for-a-file.md) is the page for both, with the
> measurements. This paragraph is dated 2026-09-03; `--palw-dump-classes` is the source that
> cannot go stale.

The response carries the DSL as the answer, the artifact (inline under
`--artifact-inline-max`, else by a handle at `GET /v1/artifacts/<derived-id>`), and a signed
`DerivedArtifactV1`. `misaka-palw-fp-rail --derive-artifact <stem>` signs it and
`misaka palw submit-object` carries it. Anyone you hand the DSL to can check the whole chain of
claims themselves, with no trust in you:

```bash
palw-derive verify --object <derived-object.borsh> --answer scene.json --artifact scene.glb \
  --output-token-ids ids.json --job-context-hash <hex> --family qwen25-a16
```

A false derivation is therefore publicly demonstrable. Stated plainly rather than hidden: on this
lane it costs the executor nothing on chain — no bond hangs on a derivation, because the chain
cannot run an arbitrary transformer and this network refuses to pretend it can. What it costs is
your name on a provenance anyone can show is wrong.
