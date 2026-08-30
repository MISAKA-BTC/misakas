# PALW-RC (testnet-11 / ConsensusV2) — launch runbook

**Status: the code path is complete and tested end to end; every remaining step is an operator
decision this repository cannot make.** A block produced by the real production path — a real
BASE-0 inference, the real ruleset, an ML-DSA-87 signature under a registered bond — is accepted by
consensus and becomes the sink
(`palw_rc_a_real_execution_produces_a_block_the_chain_accepts`).

This document is the list of things a binary cannot do for you, in the order they have to happen.

---

## 0. What testnet-11 is, and what it is not

`NetworkId` **testnet-11** is the PALW **ConsensusV2** ruleset (ADR-0042): PALW is the consensus
work, algo-6 (`POW_ALGO_ID_PALW_COMMITTED_V2`) is the attempt lane, BASE-0 is the permanent liveness
floor (ADR-0039 W6′), and the class economy is on chain (ADR-0045/0046).

It is deliberately **not** testnet-10 or testnet-11 with a flag flipped. Those networks run V1 PALW
proof-of-work, and `validate_palw_v2` refuses to install a V1 fence beside a V2 ruleset — which is
exactly why this is a new identity rather than a fork of an old one. Nodes of the three networks
reject each other at the handshake, on purpose.

Two properties worth knowing before anything else:

* **The EVM lane is ON from DAA 0 — and the lane ships in the default build (2026-08-21).**
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

## 1. The genesis card — a REGISTRY, not a bond

**Six rows, six distinct operator keys.** `derive_panel_v2` excludes a claim's own executor by
bond, by operator and by key and seats one bond per operator, so a 5-seat panel needs six — and
`BondRegistered` may not ride a transaction, so a registry too small has no later repair. A
one-row card is refused by the genesis gate (`PanelCannotBeSeated`), which is what the tool used
to produce every single time before 2026-08-22.

Two commands, split along the secrecy line: **rows are emitted where the secrets live, assembled
where they do not.**

```bash
# ON EACH OPERATOR'S HOST — two keys, secrets never leave it
misaka validator keygen --out /etc/misaka/t12-bond.key
misaka validator keygen --out /etc/misaka/t12-operator.key

# ON THE SAME HOST — one public row (two verification keys + an address payload, nothing signable)
palw-rc-genesis --emit-row --bond-index 3 \
    --bond-seed /etc/misaka/t12-bond.key --operator-seed /etc/misaka/t12-operator.key
#   the payout DEFAULTS to the bond key's own address — matured rewards are then spendable
#   by the seed that signs for them, with no second key to get wrong

# ANYWHERE — collect the six rows into a file and assemble
palw-rc-genesis --rows /tmp/t12-rows.txt
```

`--rows` puts the registry through `palw_rc_params_from_artifacts` — the same call a node makes at
boot — so an ACCEPTED card is one a node accepts, and it prints the `consensus_params_id` a node
will log. **Changing the premine (which the bond fee floats do) requires re-pinning
`PALW_RC_GENESIS`**; the M-07 guard refuses to boot on a mismatch and
`cargo test -p kaspa-consensus --lib repin::print -- --ignored --nocapture` recomputes it.

Everything else about the genesis is derived. Run it with no arguments to see what:

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
> * **`seat_count + 3` distinct operators — `seat_count + 1` is the floor, not the target.**
>   A panel seats one bond per OPERATOR and never the claim's own executor, so a 5-seat panel needs
>   six *to draw at all*. Below six no claim is ever licensed: every one voids at `BindTimeout`,
>   `safe_weight` stays zero forever, and each block's escrowed worker carve is burned.
>   **Ship eight.** At exactly six, one seat leaving eligibility — a retirement, or a slash under
>   `min_collateral_sompi` — halts every panel, and ADR-0065 D1 (seat maturity) cannot be armed at
>   all: `validate_palw_v2` refuses an armed maturity fence on a registry with no spare seat, so the
>   node will not boot. `palw-rc-genesis` prints both numbers and says which is which.
>   **A registry of clones is one operator however long it is**: each row needs its own
>   `--operator-seed`.
>
>   **And every row you ship has to be STAFFED.** The draw is liveness-blind: an offline bond still
>   takes a seat, and a panel reaches quorum by presence alone. With `seat_count = 5, quorum = 3`,
>   at most `seat_count - quorum = 2` registered bonds may be unattended before panels start
>   failing on absence. At eight registered that means **six hosts actually running the seat
>   service** — growing the registry past what the fleet can staff makes panels fail MORE often,
>   not less. Assign the two seats added on 2026-08-31 before the relaunch, or the network runs at
>   exactly zero quorum margin.
>
>   *(Corrected 2026-08-31: this used to say "`BondRegistered` may not ride a transaction, so there
>   is no later repair — the registry you ship is the registry the network has for its whole life."
>   That is false in this build. A `BondRegistered` carrier IS admitted on a live chain once it
>   locks the collateral it declares — `virtual_processor/processor.rs:4922` — so a thin registry
>   can be grown later. Two things the repair does NOT fix, which is why the genesis size still
>   matters: a chain that is already halted produces no blocks, so no carrier can land, and D1's
>   arming guard reads the GENESIS registry, so a network that grew post-genesis still cannot arm
>   the fence.)*
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

Until those are pasted, `Params::from(testnet-11)` returns the **bundle-free base identity** — a
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
cargo build --release -p kaspad
```

The `evm` feature is a **default** feature of kaspad, so the plain build is the fleet build — the
revm executor runs in-process, no separate daemon. Only a `--no-default-features` binary lacks the
lane, and it refuses to start on this network rather than failing later. `--features evm` in older
scripts is a harmless no-op.

No other feature flags. A `ConsensusV2` node needs no model runtime to **verify** — ADR-0042
Decision 4. Only a producing node links the engine, and it links it because it is producing.

---

## 3. Bring up the first node — and it must be the producer

A `ConsensusV2` network with a genesis and no producer has one block forever. The producer is
in-process:

```bash
kaspad --testnet --netsuffix=11 \
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
kaspad --testnet --netsuffix=11 --addpeer=<producer-host>:26411 --utxoindex
```

Adding seeders later is a plain edit, not a flag day: `dns_seeders` is deliberately **outside**
`consensus_params_id` — where to find peers is not a rule about blocks.

**Public reachability is its own problem and this repo has been bitten by it before.** See
[t10 public P2P reachability] in the operator notes: a proxy in front of the P2P port masks peer
IPs, and a socket bridge is not the same thing as a delegated seed name. Test reachability from
outside your own network before announcing anything.

**Start every seat BEFORE the producer.** Trace material is gossiped once and never replayed, so
a seat that is still catching up when a claim is made never sees that claim's material — and it is
*right* to file `Unavailable`, because it genuinely cannot verify. Three such verdicts are a quorum,
and the claim voids for `ProducerWithholding` with its escrow destroyed. testnet-11's relaunch
measured this exactly: the seats brought up after the producer each filed **158** `Unavailable`
verdicts covering the same claims, and then filed nothing but `Valid` once caught up. Nothing was
wrong with the network; the launch order was wrong. Bring every seat to a synced tip first, and
only then start producing.

**Every registered bond needs a node, and the registry has no spare.** `derive_panel_v2` seats
`PALW_V2_PANEL_SEATS` bonds and excludes the executor, so a six-row registry seats *all five*
non-producing bonds on *every* claim. A bond whose node is not running is not idle — it is a silent
seat, and `slash_silent_seats` takes `claim.reserved` from it on every licensed claim. It drains,
and a drained bond cannot be replaced: `BondRegistered` may not ride a transaction, so the only
repair is a flag-day relaunch. **Decide where each bond's node will run BEFORE generating its
seed**, because the seed is generated on the host that will run it and does not move afterwards.
testnet-11 had to re-mint over exactly this: bond 4's seed was on a host whose egress to the fleet
turned out to be filtered upstream.

**Reachability measured for testnet-11** (2026-08-22, from outside the fleet):

    169.58.39.220:26411
    5.104.81.23:26411

Two public entry points, which is what an announcement needs; a third fleet host peers outbound
only. Which bond each host carries is deliberately not published — on a fleet this small the map
would name the machine to attack for a panel quorum.

---

## 5. Verify the network is what you think it is

| check | how | what a bad answer means |
|---|---|---|
| the ruleset | compare `consensus_params_id` in the startup log across nodes | a node is running a different card, or an unfilled one |
| the lane | the producer's first log line names algo 6 | the bundle did not install; §1's constants are unset |
| production | `[palw-producer] produced block #N …` | see the hold reason it prints instead |
| **the lattice is turning over** | `[palw-producer] palw weight=… live_total=… final_claims=… unresolved=…` | **`live_total` at zero is the failure.** `weight` at zero is NOT — see below |
| the seats are answering | `[palw-panel] filed a "Valid" receipt for claim …` on the NON-producing nodes | no material is reaching them, or their bond holds no seat |
| the quorum reaches the chain | `[palw-panel] submitted ReceiptLicensed for claim …` | `no fee UTXO resolves` ⇒ the genesis float was spent or the card is unset |

**The weight line is the one that matters — and it must be read in two stages.** A network can
produce blocks, gossip material, file receipts and still certify nothing, and nothing else in the
log says so.

*Before the first `Final`, which is `window_challenge` (1200 DAA ≈ **40 hours** at the frozen 120 s
cadence) after the first licensed claim:* `weight` and `final_claims` are zero **by construction**,
on every healthy chain, and a young network cannot be judged by them. Read `live_total` instead —
it is the third key of the fork-choice order and carries the bounded immature contribution, so it
climbs from the first block. `live_total` rising with `weight` at zero is correct. Both at zero is
a lattice that never started: check that a funded submitter exists and that
`[palw-panel] submitted ReceiptLicensed` appears.

*After that:* `weight` must leave zero and `final_claims` must climb. A flat zero `weight` past
DAA 1200, with `unresolved` still rising, is a hash chain wearing PALW's clothes.

---

## 5b. What testnet-11 carries, and the one lane it does not

Checked against the shipped bundle rather than asserted, because "all of PALW works" is a claim
somebody should be able to verify:

| capability | on t12 | evidence |
|---|---|---|
| BASE-0 attempt lane (algo 6) | **live** | every block is a real inference; `[palw-producer] produced block` from block 1 |
| the claim lattice — panel, receipts, quorum, `Final` | **live** | `submitted ReceiptLicensed` and `live_total` climbing; `weight` follows at DAA 1200 (§5) |
| the court (step dispute, interactive ladder) | **armed, one-sided** | the ladder narrows and an arithmetic close adjudicates, and a close is now bound to the step the ladder reached. But NOTHING IN THIS TREE CONSTRUCTS `CourtDisclosed` — there is no responder — so a dispute that is opened cannot be answered, only run out. Opening costs the challenger the claim's own stake and the opening rung runs on the session budget, so an unanswerable accusation ends against the accuser rather than convicting the accused; it is a real cost on both sides, not a working two-party protocol |
| bond retirement | **live** | an owner ML-DSA-87 signature over the bond key releases the collateral lock |
| bond registration, post-genesis | **live** | `palw_bond_registration_binds_its_carrier_v2` is the collateral lock: the registration names an output of its own carrying transaction BY INDEX, with a zero transaction id — naming it by id is a hash fixed point, since the object rides in the payload and the payload is in the id — and the chain substitutes the id it observes. That output must hold at least the collateral declared and pay to the P2PKH of the payee named. The carrier proves the money, the signature (made over the zero form) proves the owner. `--palw-register-bond` builds one. Genesis bonds remain necessary only to seat the FIRST panel |
| pruned sync (serving the pruning point's PALW state) | **live** | captured at pruning-advance and served from its own store row; a node that joined by a pruned sync can hand it on |
| every lattice window | **real values** | bind 600, receipt 600, challenge 1200, court 2400 — none fenced to `u64::MAX` |
| claim retirement | **live** | `CLAIM_RETIREMENT = WINDOW_COURT` |
| free-prompt COMMITMENT (0x4a) | **open, unclienced** | consensus admits and routes the band (`palw_fp_objects_v3`), so anyone can carry one and have a claim created and licensed — but nothing in this tree BUILDS the transaction, so no first-party client offers it |
| the EVM lane | **live** | active at DAA 0 and in the default build — no `--features evm` to forget |
| lifecycle objects (0x4b) | **live** | the receipt quorum rides one |
| **free-prompt receipt SPEND (algo 7)** | **not producible** | see below |

**The one gap, stated plainly.** ADR-0044's receipt lane is fully implemented in consensus — the
header gate decodes and position-binds an algo-7 carriage, `PalwFreePromptParamsV3` is a required
part of the bundle, and a licensed free-prompt claim's weight is defined to arrive per spent
quantum at the receipt block that spends it. What does not exist anywhere in the tree is anything
that PRODUCES such a block: no `--palw-receipt-produce`, no receipt-spend builder, no miner arm.

**This is not a fence set for testnet-11.** `algorithm_id == POW_ALGO_ID_PALW_COMMITTED_V2` is
part of what ConsensusV2 *is* — the mode's own doc calls it "the only algorithm a V2 network
demands or accepts", and `validate` refuses any bundle that says otherwise. Nothing about this
deployment was narrowed to close the lane; a network that produces algo-7 blocks is a different
ruleset, and the code says so where the check lives: *when the receipt lane becomes producible this
becomes a two-sided check again — and it will be a ruleset change.*

The reason it must stay shut until then is liveness, not caution. Opening it without a producer
would hand cadence share to a lane nobody can fill, and the per-class
DAA retarget then reads the floor as an over-producer at every epoch boundary — dividing its target
by four each time until the class lottery refuses every attempt. That is the exact wedge §6 of the
[launch blockers](palw-rc-launch-blockers-2026-08-21.md) records, and it is why
`ATTEMPT_SHARE_PERMILLE` is 1000: **a lane that cannot produce holds no cadence.**

Opening it is therefore one piece of work, not two: build the receipt producer, split the share,
re-mint genesis. Until then a free-prompt commitment can be made and licensed on testnet-11, and
its licensed quanta cannot be spent.

---

## 6. What is NOT in place, and should be said out loud

> **The complete, evidenced list is
> [palw-rc-launch-blockers-2026-08-21.md](palw-rc-launch-blockers-2026-08-21.md).** This section is
> the short form; that document is the authority on current state.
>
> **Status 2026-08-22 (`9d8c7645`): the consensus blockers this section used to warn about are
> CLOSED.** Named, because each once made this runbook unable to produce a working network:
> * *pruned-sync fail-open* — a joining node ran with NO PALW rules; now it imports the
>   root-verified state at the pruning point (`e52a1234`) and refuses to run without one
>   (`0cf7ead2`), and the reply's missing wire route — which killed every pruned IBD — is fixed
>   and drilled over real TCP (`bce0f4e4`).
> * *pruning-point import* — the state transfer itself: message pair, serving flow, all-or-nothing
>   IBD fetch, root verified against the child header before any write.
> * *the unauthenticated lifecycle doors* — `ProducerDefaulted`/`ReceiptLicensed` with empty
>   receipt sets, bond retirement, forged class freezes, courts nobody authorized: all refused at
>   both layers (`40002ddd`, `4724863a`), and class registration now demands its registrant's
>   signature and a servable coverage profile (`cb131570`).
> * *no claim could reach `Final`* — the whole seat/receipt/quorum subsystem now exists
>   (`9d8c7645`): material broadcast, `--palw-panel`, and the funded submitter whose object comes
>   from the acceptance validator itself.
>
> What still stands between this runbook and a public weight-bearing network is OPERATIONAL: the
> multi-node drill (§ below), the genesis re-mint the ruleset-id change forces (settle M-02 first),
> and fleet deployment.
>
> §4's port reads **26411** and matches `NetworkId::default_p2p_port` (`network.rs:280`); an
> earlier revision said 16311, and with `dns_seeders` empty `--addpeer` is the only discovery
> path — the published number has to be the real one.


* **Third-party mining: the facts are on the wire now; no external miner has been written.**
  The blocker this bullet used to name is closed — `getPalwProducerFacts` (op 167, the full grpc
  and wRPC stack) serves the class's artifact root, its target, the pwu that target implies, the
  bond's registered key and operator id, its exposure room and a readiness verdict, all DERIVED
  (ADR-0046) so a miner cannot disagree with admission by multiplying its own. With it the wire
  path is complete: `getCurrentNetwork` → `getPalwProducerFacts` → `getBlockTemplate` → run the
  inference and the nonce search → `submitBlock` with the envelope in `palw_commitment`
  (`network_domain` is `H(network_id)`, derivable from what the node already publishes).
  What remains is not protocol: `misaminer` and `pq-miner` still branch on algo 4 and 5 only, and
  an algo-6 miner has to implement the BASE-0 engine, the carriage build and the retention
  obligation the facts advertise. Until someone writes one, the producing node is the producer —
  by absence of a client, not by absence of an interface.
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
