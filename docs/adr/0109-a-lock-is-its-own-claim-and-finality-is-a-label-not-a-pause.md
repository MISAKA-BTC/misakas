# ADR-0109 — A lock is its own claim, and finality is a label, not a pause

* Status: PROPOSED 2026-09-11, at the operator's request ("今の EVM ブリッジや送金がスムーズに liveness
  に行かない根本的原因を分析して liveness を優先にする設計に修正を行なって"). **Decisions 1–5
  IMPLEMENTED the same day** (§9), consensus-inert: no block-validity rule, no `Params` field, no
  fingerprint moves. A node takes it by an ordinary rebuild and restart; a node that never takes it
  validates every block the new nodes make.
* Builds on: [0020](0020-selected-parent-evm-lane.md) (the EVM lane executes on the selected chain;
  B's own payload is executed by its selected child), [0009](0009-dns-probabilistic-finality.md)
  (DNS finality is probabilistic and two-resource; "liveness depends on both PoW miners and PoS
  validators while the overlay is active"), [0105](0105-a-heartbeat-never-turns-a-bonded-block-red.md)
  (the 09-10 heartbeat trap, during which the bridge stood paused), [0072](0072-the-ticket-is-the-execution.md)
  (a draw is the execution; a template is fixed for the whole draw), the 2026-09-10 bridge-freshness
  fix (`dns_finality_fresh_for_bridge` measured in blue score beyond the anchor's structural
  distance), and ADR-0108's rule that a node extension is policy a build carries, never a rule the
  chain enforces.
* Amends: the producer policy and the RPC policy of kaspa-pq EVM Lane v0.4 §9.2 (deposit claims)
  and §15 (template assembly). The claim's *validity* (`validate_one_deposit_claim`) is unchanged.
* Supersedes nothing.

## 0. The sentence this ADR is

**A deposit lock that the chain has accepted is claimed by every producer, unasked, in the next
template each of them builds; the chain's finality machinery labels what is final and never decides
what is included; a transaction the chain will refuse is refused at the mempool, out loud; and the
one bound that remains is the block interval, which this ADR measures and names rather than hides.**

## 1. What was measured (2026-09-10/11, testnet-11, main `40ac431b`)

**1.1 The bridge's latency is not the chain's latency.** The lane's protocol asks the *depositor* for
a second act after the first: a `submitEvmDepositClaim` RPC, which the node can only take once the
lock's transaction is ACCEPTED on the selected chain (before that the outpoint is "absent/spent in the
virtual UTXO set"). The claim is then queued in the node's memory, gossiped on a low-priority relay
lane, and re-validated into the *next* template each producer builds. Only a chain block's payload
executes (ADR-0020), so a claim that reached one producer and not another executes only if the
first one's block becomes the chain block.

The 1,000,000 MSK deposit of 2026-09-11 (`c57eec8d…`), read from the chain:

| when (UTC) | what | DAA |
|---|---|---|
| 06:26:22 | lock tx carried by block `2a703493…` (template time) | 3372 |
| 06:54:20 | accepted by chain block `ac6ee446…` | 3373 |
| 07:07:22 | chain block `b90571fe…`, EVM payload **empty** | 3376 |
| 07:22:05 | chain block, EVM payload **empty** | 3377 |
| ≈07:30 | the wallet's claim first *accepted* by the RPC (the wallet re-asked only while its popup was open; the operator reloaded a wallet that re-asks from the background) | — |
| 07:43:12 | the first template built after the claim reached a producer — chain block at DAA 3379 carries it (template time) | 3379 |
| ≈08:15 | that block arrives and is the chain block; the credit is at `latest` (read 08:16Z: 1,000,000 MSK) | — |

**Lock in a block at 06:26Z, credit at 08:15Z: 1 h 49 min**, of which the chain needed one interval
after someone asked (07:43 → 08:15) and everything before that was waiting for the ask.

The previous day's 5 MSK deposit (`8e5315b3…`) took six minutes — claim RPC 13:27Z, credited by the
chain block at DAA 3297 (template 13:33:53Z) — because a template happened to be built just after
the relay. The difference between six minutes and an hour is not the chain; it is whether someone
asked at the right moment.

**1.2 A template is one per draw, and a draw is long.** A QWEN36 draw on the fleet's hosts takes ≈17
min; the template (and the EVM payload in it) is fixed when the draw starts (ADR-0072: the ticket is
the execution). Anything that arrives mid-draw waits for the next draw. Chain blocks on 2026-09-11
04:59–07:22Z came at intervals of 22.5, 26.2, 14.6, 23.3, 28.0, 13.0 and 14.7 min — **median 22.5 min,
mean 20.3**. A transfer needs the lock in a block (1 interval), accepted (the next chain block), a
claim in a template built after that (up to 1 draw), that block (1 interval), and that block to be
the chain block. **Under the current design that is 3–4 chain blocks after the depositor's second
act; under this ADR it is 2 chain blocks after the lock, with no second act.**

**1.3 The finality gate pauses inclusion.** `bridge_finality_is_fresh` is producer policy: when the
DNS-confirmed anchor sits more than `dns_bridge_max_anchor_distance_blue_score` (14 on testnet-11)
below the sink, or DNS is not confirmed at all, the template carries an **empty EVM payload** — no
claims, no EVM transactions, no EVM coinbase — and the claim RPC answers `EVM bridge is paused`. On
2026-09-10 the heartbeat trap (ADR-0105) kept DNS unconfirmed from 11:41Z to 13:26Z: **1 h 45 min in
which every claim was refused and no EVM transaction could be included**, while the base ledger kept
producing blocks. The partition of the same day did the same for hours. On 2026-09-11 the pause did
not fire (0 `EVM lane producer paused` lines on all eight fleet logs) — because nothing was queued
during a stale window, not because the window did not exist.

The gate never made a credit *final*: a claim landing at blue N with the anchor at N−8 is exactly as
reorgable as one landing with the anchor at N−20. What the gate did was couple the EVM lane's
*liveness* to the validators' — the thing ADR-0009 lists as a property to be stated honestly ("liveness
depends on both PoW miners and PoS validators"), not one to be desired.

**1.4 A refused transaction is refused silently.** The 2026-09-11 00:57Z lock `6388c588…` spent eight
outputs that are PALW producer bonds (the depositor's wallet chose its largest coins). The mempool
admitted it, a block carried it, and the merge refused it (`SpendsNonReleasableBond`) — correct, and
invisible: no error to the submitter, no event, the wallet waited 90 minutes before concluding
anything. The mempool knows the locked set (`palw_locked_bond_outpoints_v2` serves it over RPC) and
did not consult it.

**1.5 Nothing says where a claim is.** No RPC reports whether a lock is claimed, queued here, or
relayed; the node logs nothing when it queues one. The wallet of 2026-09-11 infers the fate of a lock
from the explorer's acceptance record and the node's sink blue score. The explorer shows no bridge at
all (the operator's third request of the day).

**1.6 The bound that stays.** Block cadence is PALW: a block is a successful draw, a draw is an
inference, and the fleet's producers draw for ≈17 min each. That is a property of the network's
hardware and its ceilings (ADR-0092, 0097), and a fast lane exists — the PALW-BASE-0 floor family
draws in seconds (chain block `ac6ee446…` at DAA 3373 is one). This ADR does not change cadence; it
removes everything the bridge added on top of it, and §8 names what would change cadence.

## 2. The causes, in one table

| cause | kind | fixed by |
|---|---|---|
| the claim is a second, depositor-initiated act, taken only after acceptance | protocol *policy* (the claim op and its validity are unchanged) | D1 |
| the claim lives in one node's memory, reaches producers by low-priority gossip, is evicted after 600 absent templates, dies with a restart | node policy | D1 |
| a stale DNS anchor empties the EVM payload and refuses every claim | producer + RPC policy | D2 |
| a spend of a locked bond is admitted to the mempool and dropped at the merge | mempool policy | D3 |
| `safe` = sink; the only finality-bearing tag is the pruning point | RPC policy | D4 |
| a template is one per draw; a draw is ≈17 min; a chain block every ≈20 min | PALW cadence | not here (§8) |

## 3. Decisions

### Decision 1 — Every accepted lock is claimed by every producer, unasked

Consensus keeps a node-local **deposit-lock index**: the set of `EVM_DEPOSIT_LOCK` outputs currently
in the virtual UTXO set, with what a claim needs (amount, EVM address, tip, timeout, the lock's DAA
score). It is staged in `commit_virtual_state` from the *same* accumulated UTXO diff the virtual set
is written from — an added output whose script parses as a lock is inserted, a removed outpoint is
deleted — so it moves with every virtual change, reorgs included, atomically with the set it
mirrors. It is rebuilt once from the virtual UTXO set on a node whose database predates it (a
marker says whether it was built), and rebuilt whenever the virtual set is replaced wholesale (a
pruning-point import).

The template path unions the mining manager's queued claims (the RPC and relay lane, kept for
compatibility) with the index's, oldest lock first, and hands the union to the unchanged
`prepare_deposit_claims` under the same virtual read lock, so every claim is validated against the
generation the template is built from — no TOCTOU, no new validity rule, the per-block caps as
before. **The depositor's second act becomes optional; a restart, a full queue, an evicted claim or
a relay that never arrived no longer decide whether a deposit completes.**

### Decision 2 — Finality is a label the reader asks for, not a pause the producer imposes

`Config::evm_bridge_finality` is `Label` (default, every network) or `Pause` (the behaviour before
this ADR). Under `Label` the producer includes claims, EVM transactions and its EVM coinbase whatever
the DNS anchor's distance, and `submitEvmDepositClaim` queues a claim whatever the anchor's distance.
`--evm-bridge-devnet-unpaused` becomes a spelling of `Label` (kept so a devnet script keeps working).
Block validity never read the gate; it still does not.

What the gate was protecting is now carried by the *reader*: Decision 4's `safe` tag. A wallet that
wants a credit it can act on externally asks for `safe`; a wallet that wants to pay gas asks for
`latest`. That is the Ethereum model during an inactivity leak — the head advances, finality waits —
and it is what ADR-0009's public-claim discipline already permits saying.

### Decision 3 — A spend the chain will refuse is refused at the mempool

`validate_mempool_transaction` refuses a transaction whose input is a PALW producer bond the
registry holds locked at the virtual tip, with the same error the merge would give
(`SpendsNonReleasableBond`), so the submitter learns at `submitTransaction` what the chain would have
told nobody. Policy: a block carrying such a transaction is still valid and the merge still skips
the transaction, exactly as before.

### Decision 4 — `safe` is the DNS-confirmed anchor

`CanonicalEvmHeads.safe` follows `DnsState.last_dns_confirmed_anchor` when that anchor carries an
EVM result, and the sink otherwise (as before). `latest` stays the sink; `finalized` stays the
pruning point. The eth RPC already resolves `safe` from the heads store, so `eth_getBalance(addr,
"safe")` and `eth_getBlockByNumber("safe")` answer with the two-resource-confirmed state without a
further change.

### Decision 5 — The claim RPC and the relay lane stay, as accelerators

`submitEvmDepositClaim` still queues and gossips a claim (a producer whose index lags — a node
mid-IBD — can still be handed one). Its refusals are the lock's own (absent, not a lock, tip over
amount, past timeout); staleness is no longer one. The wallet's background relay of 2026-09-11 stays
harmless and becomes unnecessary.

## 4. What this costs

The index is one row per unclaimed lock — on testnet-11 today, one. Staging it is one script-class
check per added virtual output (`ScriptClass::from_script`, a length-and-opcode test). Reading it at
template time is one store iteration under a lock the template already holds. The rebuild is one
pass over the virtual UTXO set, once per database. Nothing here adds a byte to a block or a rule to
its validity.

## 5. Security amendments

* **SA-1 — the index never decides validity.** Every claim the index yields passes the unchanged
  `prepare_deposit_claims` against the template's own view; an index row that is wrong (a bug) makes
  a claim that is dropped, never a block that is invalid.
* **SA-2 — the index is derived, never declared.** Rows come only from the virtual UTXO diff or from
  a scan of the virtual set; no RPC writes it. A relayed claim is still re-resolved against the live
  view (the relay flow is unchanged).
* **SA-3 — `Label` moves no finality boundary.** `finalized` is still the pruning point; `safe` is
  the DNS-confirmed anchor, which is exactly the boundary the old gate measured its distance from.
  A reader who acted on `latest` before this ADR was acting on an unconfirmed head then too.
* **SA-4 — a paused-mode operator keeps the old behaviour byte for byte.** `Pause` reproduces the
  previous template and RPC decisions, so a network that wants the coupling can set it per node.
* **SA-5 — the mempool refusal is the merge's own predicate** (the locked set the registry serves
  over RPC, read at the virtual tip), so a transaction the mempool refuses is one the merge would
  have skipped; it cannot refuse a spend the chain would accept, other than by the one-block lag
  between the tip the mempool read and the block that accepts.

## 6. Invariants the tests hold

* **I-1** A lock in the imported virtual UTXO set is claimed by the next template with *no* queued
  claim (the index path), under `Label`; under `Pause` with a stale anchor the payload stays empty,
  and with a fresh anchor it carries the claim (the pre-ADR behaviour, unchanged).
* **I-2** Staging: an added lock output enters the index, an added non-lock output does not, a removed
  outpoint leaves it; the index equals the set of lock outputs in the virtual set after a rebuild.
* **I-3** A template's index claims are ordered oldest lock first and deduplicated against queued
  claims by outpoint.
* **I-4** `safe` is the DNS-confirmed anchor when it carries an EVM result, the sink otherwise.
* **I-5** A mempool transaction spending a locked PALW bond is refused with
  `SpendsNonReleasableBond(outpoint)`; the same transaction with a free input is admitted.

## 7. What is deliberately not decided

* **The same-block claim** — a lock and its claim in one chain block would cut one more interval. It
  is a validity change (the claim view excludes the block's own body, `validate_evm_deposit_claims`)
  and therefore a fence; ADR-0108 classifies it as a ruleset change. Not here.
* **A deposit-status RPC** (`getEvmDepositStatus`: present / queued here / claimed in / refund at).
  The explorer's bridge view (built alongside this ADR from chain data) and the wallet's
  explorer-plus-sink inference cover the need today; the RPC is the right home and is a follow-up.
* **Cadence.** Floor-family producers (PALW-BASE-0, seconds per draw) raise the block rate without a
  rule change; whether the fleet should run more of them is an operations decision with an economics
  side (the floor's share) that ADR-0054/0107 govern.
* **Mainnet's `evm_bridge_finality` default** is `Label` like every network; the launch preset can say
  `Pause` if the launch wants the coupling.

## 8. Number hygiene and implementation record

`docs/adr/README.md` on `main` at `40ac431b` says the next free number is 0108; 0108 is resident on
`feat/adr-0108-extension-envelope` (this operator's session, same day, unpushed) and its README says
0109. This ADR takes **0109**; **the next free number is 0110.**

Implementation, 2026-09-11, on `feat/adr-0109-bridge-liveness` — the measured results are in §9.

## 9. Measured results

Implemented 2026-09-11 on `feat/adr-0109-bridge-liveness` (from `main` `40ac431b`), nothing pushed.

**Where it lives.** `EvmDepositLockRecord` and `EvmBridgeFinalityPolicy` in `consensus/core/src/evm/mod.rs`;
`Config::evm_bridge_finality` + `evm_bridge_finality_effective()`; the store `DbEvmDepositLockStore`
(`consensus/src/model/stores/evm.rs`, DB prefixes **228** rows / **229** built-marker — 254 and 255 were
taken, which the compiler said as E0081); in `processor.rs`: `stage_evm_deposit_locks` (called from
`commit_virtual_state` next to `write_diff_batch`), `rebuild_evm_deposit_lock_index` (from `init` when the
marker is absent and from `import_pruning_point_utxo_set`), `with_indexed_deposit_claims` (in the template
path, under the virtual read lock, before `prepare_deposit_claims`), the `safe` head in
`update_evm_canonical_heads`, and the mempool refusal in `validate_mempool_transaction_impl`
(`palw_mempool_locked_bonds`, memoised per registry tip and DAA); `kaspad --evm-bridge-finality
label|pause`; the claim RPC's staleness refusal now only under `Pause`.

**Tests** (`CARGO_TARGET_DIR` private to the branch; `MISAKA_PALW_POW_FIXTURE=1`):

| suite | result |
|---|---|
| `cargo test -p kaspa-consensus --features evm` | **311 passed, 0 failed**, 7 ignored (before I-4 was added; I-4's test passes on its own) |
| `cargo test -p kaspad -p kaspa-rpc-service -p kaspa-mining` | **191 passed, 0 failed** |
| new: `evm_producer_claims_every_accepted_lock_unasked` (I-1), `evm_template_claims_oldest_lock_first_and_once` (I-3), `evm_deposit_lock_index_follows_the_virtual_utxo_diff` (I-2), `first_locked_input_names_the_locked_bond_spend` (I-5), I-4 inside `evm_active_chain_executes_persists_and_moves_heads` | pass |
| `cargo clippy -p kaspa-consensus -p kaspad -p kaspa-rpc-service -p kaspa-database --features evm --tests --no-deps -D warnings` | clean |
| `cargo clippy -p kaspa-consensus-core` | 6 errors — **the same 6 on `main` `40ac431b`** (measured side by side; none in lines this ADR touched) |
| `rustfmt --edition 2024 --check` on every touched `.rs` | clean |
| `bash scripts/ci-gates.sh --group fast` | 6/6 green |
| `python3 scripts/check-repin-enumeration.py` | 7 unclassified — `main`'s baseline, unchanged |

**Mutations** (each applied alone to `processor.rs`, then restored byte for byte): no index union in the
template → I-1 and I-3 fail; `Label` ignored (the old pause) → I-1, I-3 and the pipeline test fail; `safe`
left at the sink → I-4 fails; the diff not staged into the index → I-2 fails. **4 of 4 caught.**

Two fixtures had to change, and both changes are the point of the ADR, not accommodations:
`set_fresh_dns_finality` named an anchor hash no header exists for — since the 2026-09-10 blue-score
freshness fix such an anchor reads as stale, so the one test that depended on "fresh" was passing only
because nothing else had asked; it now names genesis. The pre-existing queue-path test asserts
"a stale anchor keeps the payload empty", which is `Pause`'s behaviour; it now says `Pause` explicitly.

**Not verified here:** the fleet does not run this build yet (a restart-only rollout: the index builds
itself on first start, and old-build peers validate every block a new build makes); the index's
rebuild over a large UTXO set (testnet-11's is small; on a mainnet-sized set it is one pass at the first
start); a live claim made by the index on testnet-11 (it needs the rollout).
