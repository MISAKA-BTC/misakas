# ADR-0063 — The operator's half of the protocol is missing, and one gap locks money in

> Renumbered 0060 → 0063 at the 2026-08-30 branch merge: 0060 was taken the same day by the
> liveness doctrine, and 0061/0062 by the zero-seat genesis and the DA court. Same content.

Status: **Proposed** (2026-08-30).

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
