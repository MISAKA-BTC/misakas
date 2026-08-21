# PALW-RC (testnet-12 / ConsensusV2) — launch runbook

**Status: the code path is complete and tested end to end; every remaining step is an operator
decision this repository cannot make.** A block produced by the real production path — a real
BASE-0 inference, the real ruleset, an ML-DSA-87 signature under a registered bond — is accepted by
consensus and becomes the sink
(`palw_rc_a_real_execution_produces_a_block_the_chain_accepts`).

This document is the list of things a binary cannot do for you, in the order they have to happen.

---

## 0. What testnet-12 is, and what it is not

`NetworkId` **testnet-12** is the PALW **ConsensusV2** ruleset (ADR-0042): PALW is the consensus
work, algo-6 (`POW_ALGO_ID_PALW_COMMITTED_V2`) is the attempt lane, BASE-0 is the permanent liveness
floor (ADR-0039 W6′), and the class economy is on chain (ADR-0045/0046).

It is deliberately **not** testnet-10 or testnet-11 with a flag flipped. Those networks run V1 PALW
proof-of-work, and `validate_palw_v2` refuses to install a V1 fence beside a V2 ruleset — which is
exactly why this is a new identity rather than a fork of an old one. Nodes of the three networks
reject each other at the handshake, on purpose.

Two properties worth knowing before anything else:

* **The EVM lane is ON from DAA 0, and that makes `--features evm` mandatory for the fleet.**
  testnet-11 carries the lane and the RC is the network t11's traffic moves onto, so the RC carries
  it too. `MAINNET_PARAMS` never activates it, so the RC and mainnet differ here — a known,
  deliberate difference rather than an inherited accident.
  **`build_block_template` cannot construct a valid template without the feature.** A non-evm
  binary is now refused at STARTUP with the rebuild command rather than panicking at its first
  template, which is after boot, after IBD, and after an operator has every reason to think the
  node is fine.
* **`dns_seeders` is empty and cannot be filled from code.** Inheriting the other testnets' seeders
  would be worse than empty: those records answer with testnet-10/11 nodes, which this network
  rejects, so discovery would look configured and find nobody.

---

## 1. The genesis card — three facts code cannot mint

Everything else about the genesis is derived. Run:

```bash
cargo run --release -p misaka-palw-base0 --bin palw-rc-genesis
```

It prints what this build derives, and this is the same on every machine — nothing to host, nothing
to mirror:

```
base0 seed          0x50414C575F524330
geometry            4 layers, hidden 256, ffn 512, heads 4x64, vocab 4096, n_ctx 512, tile 64
execution_class_id  c185df95388739dc549777a9ca43866ddf773f1c84df77479a9eb59ba8d1d2b2…
artifact_root       204fea7788fd4c2dc20812d0c07e0aa3b9edea60baa7c89f741bf995bc6044ab…
One block's work — canonical job (8 prefill, 4 decode):
  step leaves         7900
  wall time           0.038 s
```

`artifact_root` used to be described as "the one input code cannot mint". Half of that stopped
being true: `palw_base0_profile`'s own doc says *BASE-0 has no file: it is a specification*, and a
specification's artifact can be **produced** by a rule. Every weight byte derives from one pinned
seed, so the root is re-derivable by anyone in any language rather than being a 4.5 MiB blob every
participant must be handed a correct copy of.

The half that really cannot be minted is the **bond**:

| fact | what it is | who makes it |
|---|---|---|
| `--bond-index` | which premine output backs the genesis bond (0..=40) | you |
| `--bond-pubkey` | the ML-DSA-87 **verification** key that signs attempts under it | `misaka-cli` |
| `--operator-pubkey` | the operator identity key — panel dedup is keyed on it | `misaka-cli` |
| `--payout-payload` | the 64-byte P2PKH-ML-DSA-87 owner payload matured rewards pay to | you |

> ### It is a REGISTRY, and one bond is not a network
>
> `PALW_RC_GENESIS_BONDS` is a **list**, and the genesis gate refuses a list that cannot run a
> chain. Two properties, both checked at assembly rather than discovered at block three:
>
> * **`seat_count + 1` distinct operators.** A panel seats one bond per OPERATOR and never the
>   claim's own executor, so a 5-seat panel needs six. Below that no claim is ever licensed: every
>   one voids at `BindTimeout`, `safe_weight` stays zero forever, and each block's escrowed worker
>   carve is burned. `BondRegistered` may not ride a transaction, so there is no later repair — the
>   registry you ship is the registry the network has for its whole life.
>   **A registry of clones is one operator however long it is**: each row needs its own
>   `--operator-seed`.
> * **Collateral that outlasts the bind window.** A claim holds its reservation for `window_bind`
>   (600 DAA) and DAA advances only when blocks are produced, so a ceiling admitting fewer
>   concurrent claims than the window is long is a deadlock with no timeout. The requirement is
>   derived (`palw_v2_collateral_for_bind_window_v1`) and the gate names the number if you are
>   short.
>
> The shipped bundle failed both: one bond, 400,000 sompi, `supported: 2`. It would have made two
> blocks and stopped.

**Generate the keys with `misaka-cli` and keep the secrets there.** `palw-rc-genesis` generates no
keys and touches no key material: a tool that minted a key would be minting an identity, and the
whole point of a bond is that somebody holds one. Pass only verification keys.

Then run it again with all four, and it prints the paste-ready block for
`consensus/core/src/config/params.rs`:

```
PALW_RC_GENESIS_ARTIFACT_ROOT     (already pinned)
PALW_RC_GENESIS_BOND_INDEX
PALW_RC_GENESIS_BOND_PUBKEY
PALW_RC_GENESIS_OPERATOR_PUBKEY
PALW_RC_GENESIS_PAYOUT_PAYLOAD
```

Until those are pasted, `Params::from(testnet-12)` returns the **bundle-free base identity** — a
hash-only chain with the RC genesis — and `kaspad` says so at startup. That is the shipped state on
purpose: a placeholder key would be an identity nobody holds the secret for, which looks like a
network and is not.

A filled card and an empty one differ at `consensus_params_id()`, so which one a node is running is
a fact the **handshake reports** rather than something an operator has to be trusted to have done.

> A card that is set but does not assemble panics at startup with the genesis gate's own message.
> Falling back to the base would put the node on a chain it cannot join and tell it nothing.

---

## 2. Build

```bash
cargo build --release -p kaspad --features evm
```

**`--features evm` is required** — see §0. A binary without it refuses to start on this network
rather than failing later.

No other feature flags. A `ConsensusV2` node needs no model runtime to **verify** — ADR-0042
Decision 4. Only a producing node links the engine, and it links it because it is producing.

---

## 3. Bring up the first node — and it must be the producer

A `ConsensusV2` network with a genesis and no producer has one block forever. The producer is
in-process:

```bash
kaspad --testnet --netsuffix=12 \
  --palw-produce \
  --palw-producer-key=/path/to/bond-seed.hex \
  --palw-producer-bond=<txid>:<index> \
  --palw-producer-pay-address=<ML-DSA-87 P2PKH address> \
  --utxoindex
```

* `--palw-producer-key` is the **32-byte hex seed** whose verification key you registered in §1.
  It is loaded with the same hardened path the validator uses: owner-only permissions at creation,
  symlinks refused, fail closed.
* `--palw-producer-bond` is the same outpoint the genesis card names.
* `--palw-producer-pay-address` **must** be ML-DSA-87 P2PKH. A legacy or ECDSA address puts a
  non-PQ script in the coinbase; the block is dead on arrival and its reward poisons descendants'
  fan-out. The producer refuses at startup rather than at the first template.

The producer holds and says why when it cannot produce — a wrong key, a spent epoch budget, or a
full exposure ceiling are each reported by name. **A held producer is not a broken one**: the
exposure ceiling is the ruleset's rate limit on how many claims one bond may have open at a time,
and it clears as claims resolve.

### What a block costs

One template is one job. The job is anchored on the template's **pre-pow hash** — not on the
challenge — so:

* one inference per template (~38 ms on the RC floor), then
* a free nonce grind: per nonce the attempt is rebuilt and hashed, and `l1_tag_v2` is a CPU
  expansion by design so this stays a nonce search rather than an inference search;
* one ML-DSA-87 signature, on a hit only.

A producer can still grind jobs by reshuffling the block it builds, which moves the pre-pow hash —
and that costs a full inference per try, which is the price the design means to charge.

---

## 4. Bring up the rest, and connect them

`dns_seeders` is empty (§0), so nodes find each other by `--addpeer`. `kaspad` WARNS at startup when
it has neither seeders nor explicit peers rather than sitting alone in silence.

```bash
kaspad --testnet --netsuffix=12 --addpeer=<producer-host>:16311 --utxoindex
```

Adding seeders later is a plain edit, not a flag day: `dns_seeders` is deliberately **outside**
`consensus_params_id` — where to find peers is not a rule about blocks.

**Public reachability is its own problem and this repo has been bitten by it before.** See
[t10 public P2P reachability] in the operator notes: a proxy in front of the P2P port masks peer
IPs, and a socket bridge is not the same thing as a delegated seed name. Test reachability from
outside your own network before announcing anything.

---

## 5. Verify the network is what you think it is

| check | how | what a bad answer means |
|---|---|---|
| the ruleset | compare `consensus_params_id` in the startup log across nodes | a node is running a different card, or an unfilled one |
| the lane | the producer's first log line names algo 6 | the bundle did not install; §1's constants are unset |
| production | `[palw-producer] produced block #N …` | see the hold reason it prints instead |
| the floor is producing | `palw_producer_facts_v2().epoch_produced_blocks` moves | the epoch budget or the ceiling is the reason |

---

## 6. What is NOT in place, and should be said out loud

* **Third-party mining does not work.** `misaminer` and `pq-miner` branch on algo 4 and 5 only.
  Making an external miner possible needs the class target, the pwu and the bond registration on
  the RPC wire — a protocol change, and its own piece of work. Until then the producing node is the
  producer.
* **The court's other two legs.** `full_logits_trace_root_v2` hashes f32 rows and
  `PalwActivationTapProfileV1` requires a non-empty f32 tap list; BASE-0 is an integer class and
  taps nothing, so it commits integer roots of its own in those two slots. The arithmetic court —
  the one that adjudicates a tile — is complete and round-tripped. A court that one day adjudicates
  the logits leg has to know which scheme a class uses.
* **Gate 3 (the second class) is not open.** The RC ships one weight-bearing class, the floor.
  Qwen is registered weightless until its calibration justifies activation
  (`palw-qwen-class-static-ptq-and-rc-boot`).
* **The soak clock resets on redeployment.** Every previous rollout learned this the hard way: a
  soak measured in "days since launch" restarts the moment you push a binary. Measure it against
  something the deployment does not touch.

---

## Appendix — the facts a producer reads, and why they are handed over whole

A V2 attempt is refused unless six fields equal values the chain already holds: the class's
registered artifact root, the class target the per-class retarget maintains, the pwu
`palw_pwu_v1` derives from that target, the bond's registered key, the operator id minted at
registration, and — as a bound — the exposure the bond still has room to back.

`palw_producer_facts_v2` reads all six from the state store at virtual's selected parent, using the
same accessors admission uses, and hands them over **derived**. Exposing the ingredients and letting
a producer multiply them would give every producer an independent chance to disagree with
admission — the exact shape of the correspondence defects this codebase has found repeatedly.
Derive, never declare (ADR-0046).

The DAA score in those facts is the **candidate's**, not the tip's: admission's epoch index comes
from the candidate, so a producer handed the tip's would check its budget against the wrong epoch at
every boundary.
