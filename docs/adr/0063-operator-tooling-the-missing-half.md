# ADR-0063 — The operator's half of the protocol is missing, and one gap locks money in

> Renumbered 0060 → 0063 at the 2026-08-30 branch merge: 0060 was taken the same day by the
> liveness doctrine, and 0061/0062 by the zero-seat genesis and the DA court. Same content.

Status: **Proposed** (2026-08-30).

> **Standing (index reconciliation, 2026-09-02).** Decisions 2 and 3 are built and were verified
> against testnet-11 (the body's corrections record it); `misaka bond capability` followed the same
> shape under [ADR-0071](0071-the-attempt-lanes-price-and-the-tickets-bound.md) Decision 3, and
> `misaka palw submit-object` / `palw-certify` under [ADR-0075](0075-certification-is-a-consensus-object.md)
> Decision 7. Decision 1's BIP39 half and Decision 4 (the `miner` subcommand) remain open as
> written. Map: [`README.md`](README.md).

> **Security amendment appended (2026-09-02)** — see the last section: seeds never on argv or in the environment; role-separated BIP39 derivation; retirement signed under the network domain; the `miner` subcommand is deleted rather than shipped; `ClassFrozen` has one author — the transition.

## How this was found

A user pasted a 24-word BIP39 mnemonic and asked to send from it. `misaka key` could not: it
generates and it shows an address, and there is no third verb. That is not a missing convenience
— **a key this tree cannot import is a key this tree cannot spend**, so every wallet, backup and
second machine outside it is unreachable. Pulling that thread found a worse one.

## The defect that locks money in

`PalwConsensusObjectV2::BondRetireRequested` is **verified and never built**.

```
consensus/src/pipeline/virtual_processor/processor.rs:4739   verifies the ML-DSA-87 signature
                                                              over palw_bond_retirement_message_v2
kaspad/src/palw_panel.rs:2355   PalwConsensusObjectV2::BondRetireRequested { .. } => "BondRetireRequested",
```

That second line is the *only* other reference in the tree, and it is a label in a log formatter.
No CLI subcommand, no `kaspad` flag, no library helper constructs the object. Grepping every
consensus object for construction-side references puts the shape beyond doubt:

| object | construction-side references |
|---|---|
| `BondRegistered` | 2 |
| `ClassRegistered` | 3 |
| `CourtOpened` | 4 |
| ... | ... |
| **`BondRetireRequested`** | **1 — a log label** |
| **`ClassFrozen`** | **1 — a log label** |

Meanwhile `docs/testnet11-join-mining.md` §6 tells an operator:

> The collateral is locked in the output the registration names and **is reclaimable at your pay
> address once the bond is retired** (an owner ML-DSA-87 signature over the bond key releases it).

The consensus rule is real and the sentence is true about the rule. It is false about the
software: nothing an operator can run produces that signature in that object. **PALW collateral
goes in and does not come out.** On testnet-11 the smallest carryable bond is 8,333,924 sompi, and
the 2026-08-30 economics pass proposes floors above it (ADR-0061 sets 10,000 MSK a seat) — every one of those is currently
a one-way door.

`ClassFrozen` has the same shape. Whether that matters depends on who is supposed to freeze a
class and by what authority, which this ADR does not decide; it is recorded because the same grep
found it and a second verify-only object is a pattern rather than an accident.

## The asymmetry underneath

The VLT lane has a complete operator surface. The PALW lane does not:

| | VLT validator | PALW producer |
|---|---|---|
| generate a key | `validator keygen` | `key gen` |
| **import a key** | **—** | **—** |
| see status | `validator status` | `--palw-dump-classes` (a node flag) |
| bond | `validator bond` | `kaspad --palw-register-bond` |
| **unbond / retire** | `validator unbond` | **nothing** |
| balance | `validator balance` | `wallet utxo list` |

Both lanes lack import. Only PALW lacks an exit. The two were built by different passes and the
PALW side stopped at "an operator can start producing", which is where a launch checklist ends and
not where an operator's needs do.

## Decision

### D1. `misaka key import` — read a key the tree did not generate

Takes an existing secret and writes the 0600 file the rest of the CLI already consumes. Two source
forms, because two exist in the wild:

* `--hex-stdin` — a 32-byte ML-DSA-87 seed, the format `key gen` writes. Covers backups, second
  machines, and keys generated on an air-gapped host.
* `--mnemonic-stdin` — a BIP39 phrase, **if and only if** a derivation from BIP39 to this tree's
  ML-DSA-87 seed is specified and shared with the web wallet. It is not today: `wallet-core-bundle.js`
  on `wallet.misakascan.com` carries a bip39 implementation and this tree carries none, so the two
  have no agreed derivation and an import that guessed one would hand back a different address in
  silence. **Specifying that derivation is a prerequisite of this half, not part of it.**

Never an argument, always stdin or a file — `misaka key`'s own help already promises "the secret is
never a CLI arg", and this is the verb most likely to break that promise.

### D2. `misaka bond retire` — build the object the chain already accepts

Signs `palw_bond_retirement_message_v2(network_domain, bond)` under the bond key and submits a
`BondRetireRequested` carrier. This is the smallest change that turns a written promise into a
runnable one, and the verification side needs no work: it has been waiting for a caller.

It must refuse while the bond has live claims — a retirement that outran a claim's data obligation
would take the collateral out from under a court — and say so with the claim ids rather than a
generic error.

**Implementation note, from running it against testnet-11 (2026-08-30).** That refusal is harder to
wire than it reads, and it was wrong twice before it worked:

1. `getPalwProducerFacts` takes the bond as **three** fields — a bare 128-hex txid, the index, and
   `with_bond` — not the `<txid>:<index>` string every operator flag spells it with. Passing the
   joined form with `with_bond: false` (the field whose own doc says *"false when
   `bond_transaction_id` is not to be read at all"*) left `bond_known` permanently false.
2. Even correctly shaped, an **empty `class_id` returns before the bond is looked at** — a
   deliberate arm so a wallet can skip collateral without knowing a class. The reserved exposure is
   therefore unreachable unless a class is named, and **no RPC lists classes**, so the CLI cannot
   discover one. `bond retire` takes `--class-id`, and without it **refuses**: a guard that steps
   aside when it cannot see is worth less than no guard, because it still reads like one.

Both arms are now verified against the live chain — with a real class id the command reports the
producer bond's `53108120` of reserved exposure and refuses. The unit the refusal quotes is
**reserved exposure**, not a claim count: it is the same number admission checks against the
ceiling, so the CLI and consensus agree by construction.

The API gap is worth recording on its own: **there is no way to ask this chain about a bond without
naming a class, and no way to list classes.** The exposure belongs to the bond, not to the class,
so the coupling is incidental — and it is what made a guard silently unreachable.

### D3. `misaka bond status` — the outpoint, from the chain

Today the bond outpoint appears exactly once, in a log line the runbook tells operators to keep:

> **That line is the only place the bond's outpoint appears.** ... the node does not store it
> anywhere else. Keep it.

An operator who loses the line has a funded, working bond they cannot name — and `--palw-producer-bond`
takes the outpoint. `getPalwProducerFacts` already returns the locked bond outpoint (the wallet
calls it to avoid spending collateral), so this is a formatting job over an RPC that exists.

**Correction, from running it on testnet-11 (2026-08-30). It is not a formatting job, and the first
implementation answered the wrong question twice.**

1. **The locked set is not a bond set.** `rpc/service/src/service.rs:763` unions consensus-locked
   collateral with *this node's own reserved PALW funding outpoints*, so a running producer's panel
   fee outpoint is reported as locked and read as collateral. On ibm it did exactly that, and the
   outpoint it offered would have named a bond the registry has never heard of. The CLI cannot
   separate the two from that call, so it no longer claims to: the heading is `locked:`, the JSON
   field is `locked_outpoints`, and the output says how to settle which is which.
2. **A key's bond need not sit at the key's address.** A genesis bond's collateral is posted by the
   main wallet while the bond is registered to the operator's key (ADR-0059); a sponsored
   registration is the same shape. Scanning UTXOs at the key's own address therefore reports **none**
   for a key that holds a live, working bond — which is precisely the operator this decision exists
   for. Measured: seat 2's key owns genesis bond `…:2` and the address scan found nothing.

**Ownership is a property of the registry, so ask the registry.** `bond status` now takes
`--class-id` and, with it, walks the network's locked set and keeps the outpoints whose
`bond_registered_pubkey` is this key's. Without it the command says the check was not run rather than
printing a "none" it cannot stand behind.

The lesson worth more than the fix: **an absence produced by looking in the wrong place is
indistinguishable from an absence in the world**, and this command's whole job is to tell an operator
that difference.

### D4. `misaka miner` must not forward to a binary that is not there

`misaka miner` forwards to `kaspa-pq-miner`, which is not installed on the fleet host this was
measured on. Either ship it or remove the subcommand: a command that exists and cannot run teaches
an operator that the tool is unreliable, in the one place they most need to trust it. (Note that
per §0 of the join runbook a hash miner cannot mine this network at all, which makes the case for
removal stronger than the case for shipping.)

## What this does not propose

**A wallet.** `wallet send`, `utxo list` and `utxo consolidate` cover what an operator does with
funds, and the gap above is not "more wallet" — it is that a key from elsewhere cannot enter and a
bond cannot leave.

**Changing the mnemonic story.** The web wallet's BIP39 and this tree's ML-DSA-87 seeds are two key
systems that do not currently meet. D1's second bullet is conditional on that being resolved, and
resolving it is a design decision about derivation, not a CLI feature.

## Consensus impact

**None.** Every rule these commands need is already in consensus and already tested — D2 constructs
an object the pipeline verifies today, D3 reads an RPC that exists today, D1 touches no chain rule.
That is the reason to do them: the protocol half is finished and the operator half is not, so this
is entirely tooling and carries no fingerprint, no version bump and no re-mint.

## Priority

D2 first, and not because it is the largest. Every other gap costs an operator time; that one costs
them their collateral, it is promised in writing to work, and the amount at risk grows with every
bond floor the economics pass raises.

## Security amendment (2026-09-02) — hardening before the open decisions are built

**SA-1 — Key import never touches argv or the environment.** `misaka key import` reads the seed
from stdin or from a `0600` file named by path, refuses to write a key file that is not `0600`,
zeroizes its buffers and never echoes the seed. A seed on the command line is in `ps` and shell
history on every host; a seed in an environment variable is inherited by every child — including,
until ADR-0079 R-01 lands, the model worker.

**SA-2 — A shared BIP39 derivation is domain-separated per role.** If Decision 1's BIP39 half is
built, the derivation string carries the role — bond key, operator key, payout key, wallet — as a
pinned constant with a known-answer test, so one mnemonic never yields the same key in two roles.
A payout key that equals a bond key turns a wallet compromise into a slashable-collateral compromise.

**SA-3 — Retirement is signed under the network domain.** `BondRetireRequested` is verified against
the bond's own key; its message must include the network domain (as `BondCapabilityDeclared` and
`DerivedArtifactV1` do), so a retirement signed on a devnet is not replayable on the RC.

**SA-4 — Decision 4 resolves to deletion.** A `misaka miner` that forwards to an absent binary
found on `PATH` executes whatever a writable `PATH` entry holds; a hash miner on a network whose
only hash lane is the fee-only heartbeat would be a tool for earning nothing. Remove the subcommand.

**SA-5 — `ClassFrozen` has an author, and it is the transition.** The ADR notes a verify-only
object with no constructor and leaves "who may freeze a class" undecided. Decide it before a
constructor appears: a class is frozen only by the transition on a court outcome (`Unadjudicable`,
ADR-0037 I10 as carried by ADR-0038), never by an operator-signed object. An operator freeze is a
governance key on a chain that has none.

## Record (2026-09-05)

* **D1's BIP39 half is not built, by the ADR's own condition.** `--mnemonic-stdin` exists "if and
  only if a derivation from BIP39 to this tree's ML-DSA-87 seed is specified and shared with the
  web wallet", and no such derivation is specified in this tree or shared by the wallet as of this
  date. It is not an implementation gap here; it is a specification the wallet must publish first.
* **D4 is closed by deletion.** `misaka miner` is gone (SA-4), and `misaka-cli/src/main.rs` pins
  its absence with a test ("`misaka miner` is back — SA-4 deleted it").
