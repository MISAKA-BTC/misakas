# testnet-21 → testnet-22 migration

As of 2026-08-02, **`testnet-22` (`evm-genesis-palw`) is the only publicly operated MISAKA
testnet**. `testnet-21` is retired: its seeders serve testnet-22, its nodes are stopped, and its
datadirs were deleted at the cutover. `testnet-20`, `testnet-200` and `testnet-10` remain
deprecated/retired as before.

## Why testnet-21 was replaced

Two things landed together, and only one of them could have been a flag day.

**1. A third-party static audit found two Critical fork-relativity defects in the PCPB clauses,
and the fixes move leaf acceptance.** Clauses 11, 12 and 13 resolved their context — the beacon
seed `R_E`, the provider-snapshot commitment, the post-commit draw seed — out of *epoch-keyed*
stores. Those stores are idempotent-by-VALUE, and their own contract says so: the stored value is
"a function of the chain that first closed this epoch". Two forks that both close epoch `e`
legitimately write different values under one key, and whichever committed last answered for
**every** reader, including a clause validating a block on the other fork. Leaf acceptance
depended on fork receive order, sink-search order and reorg path — a consensus split with no
transaction-level cause.

All three reads now walk the *candidate's own* closed-epoch chain from its selected parent. The
epoch-keyed stores keep a narrower job: they answer only once a walk runs off the retained rows,
i.e. below the pruning point, where the value is final on every honest node and no fork can
differ. That is also what keeps the pruning-snapshot carry meaningful for a pruned joiner.

The second finding (C-02) is that the PCPB band `0x45` had a recognizer, a payload type, a
validator, a registry writer, a retention sweep, a pruning carry and two consensus clauses reading
its registry — and no routing arm. Every `PalwACommitV1` was refused as `SubnetworksDisabled`, so
the registry could never hold a row, so the **self-serial dispatch kind was structurally dead**
while reading as fully enforced. It is routed now, and a leaf may only name an anchor whose
acceptance epoch is already buried beyond the deepest legal reorg — the property that lets an
anchor-keyed registry be read safely without a per-candidate chain of its own.

Both fixes change which leaf chunks a node accepts. Rolling them onto a live network splits it —
testnet-21 demonstrated exactly that on 2026-08-02 with a smaller acceptance change. A flag day
was possible in principle, but see (2).

**2. The datadir could not survive anyway.** `LATEST_DB_VERSION` moves 16 → 18. Every active chain
block now carries two new block-keyed rows (the beacon state gains its closed-epoch chain; the
PCPB snapshot chain is a new prefix). v17 was a positional Borsh break; v18 is subtler and worse
to ignore — a v17 history *decodes fine* and simply has no chain rows, so every walk would
conclude "buried" and fall back to the shared epoch key. That is the pre-fix behaviour restored
silently, with no error anywhere, which is precisely what a version bump exists to prevent.

Given a forced resync either way, a re-genesis onto a new suffix is cheaper and safer than a flag
day on a network whose datadirs all had to be rebuilt regardless.

**testnet-22 additionally activates the EVM lane from block 0** rather than at a mid-chain fence,
which is the reason the preset was created before the audit findings arrived.

## What changes for an operator

| | testnet-21 (old) | **testnet-22 (new)** |
|---|---|---|
| Select with | `--testnet --netsuffix=21` | `--testnet --netsuffix=22` |
| P2P port | `26531` | **`26541`** |
| Genesis | `pcpb-palw` | **`evm-genesis-palw`** |
| Params identity | `11d3afa3…e8af9cab` | **`ec583c2e…69316a3f`** |
| Discovery | `seeder1.misakascan.com`, `seeder3.misakascan.com` | unchanged (both now serve testnet-22) |
| PALW lane | active, algo-4 fenced at DAA 2,000,000 | active, **algo-4 open from block 0** |
| Self-serial dispatch | structurally dead (`0x45` unrouted) | **live** |
| EVM lane | activates at DAA 6,500,000 | **genesis-active** |
| DB version | 16 | **18** |

- **Fresh datadir required.** No state carries across a re-genesis, and the DB version moved
  besides. Do not point a testnet-21 datadir at testnet-22 — the node will offer to delete it.
- **`--features evm` is mandatory.** The EVM lane is active from block 0, so a non-EVM build
  cannot follow the chain at all. Build with:
  ```bash
  cargo build --release -p kaspad --bin kaspad --features evm
  ```
  The startup log must read `EVM lane (ADR-0020): ACTIVATES at DAA score 0 on testnet-22`.
- **Check the identity line.** `kaspad` prints `consensus params identity ec583c2e…` at startup
  and `getInfo` reports it as `consensusParamsHash`. Compare it across your nodes before trusting
  a mesh; a mismatch means someone is running different consensus parameters.
- Bound EVM state growth with `--evm-storage-profile=compact`. Node-local and consensus-neutral.

## For producers

The two PCPB dispatch kinds have different admission geometry on this network.

**Beacon-assigned (external)** is unchanged from testnet-21: the first mintable
`registered_epoch` is `k + Δ = 4` (design memo §10.2). The early epochs are structurally
algo-4-empty — that is fail-closed, not a fault.

**Self-serial** is newly usable, and carries one additional rule. A leaf may only name an
`a_commit` anchor whose on-chain acceptance epoch is buried at least
`ceil(max_reorg_horizon_blocks / palw_epoch_length_daa) + 1` epochs below the epoch of the block
validating it — **4 epochs** on this preset. Combined with clause 11's `registered − issued ≤ w`
(w = 6), the usable registration window for a self-serial leaf is
`[anchor + 4, anchor + 6]`: submit the `0x45` anchor transaction, wait four epochs, then register
the batch. Registering earlier fails closed with an "is not yet buried" rejection, not a retryable
error.

The preset preflight asserts `burial ≤ w`, so a parameter change that would make this window empty
— and silently kill the dispatch kind again — fails at startup rather than in production.

## Flag day: DAA 70,000 — the proposal-③ walk-down at the beacon coordinate

Set on launch day, ~1.6 hours after the cutover. **Every node must be on the
`testnet-main-<sha>` release that carries it before DAA 70,000.** After the fence an un-upgraded
node derives a different `R_E` and splits off.

What it fixes: `1d11021d` corrected the DNS confirm-latch race — confirm the newest *attested* epoch,
not the newest *ready* one — but only at the virtual-tip singleton. The beacon coordinate kept the
original form, and an epoch only becomes attestable once it is ready, so demanding the ready epoch
itself be attested is a race the chain wins essentially always. The anchor never latches, no epoch
is ever Healthy, the beacon seed stays frozen at zero, and **the algo-4 lane is closed
network-wide** — observed here as 103 consecutive degraded epochs from genesis, briefly broken by a
1-in-108 boundary coincidence and then closed again.

Below the fence the old rule still applies, so every block mined before it replays byte-identically;
only the params digest moves (identity `f88c33dc…` → `ec583c2e…`, clause (a)).

## Operator checklist

1. Stop your testnet-21 node.
2. Delete (or move aside) its datadir. Nothing in it is reusable.
3. Rebuild or re-download `kaspad` **with `--features evm`**.
4. Start with `--testnet --netsuffix=22`. Discovery is unchanged; the seeders already answer for
   testnet-22.
5. Confirm the startup log shows the `ec583c2e…` identity and the genesis-active EVM line.
6. `misaka` CLI, `kaspa-pq-validator` artifacts and MTP scoring all target testnet-22. MTP scope is
   name-keyed, so testnet-20/21 epoch ledgers stay verifiable under their own names.

## Validator operators: attest, or the lane closes

Repeating the lesson testnet-21 paid for on 2026-08-02, because it applies from block 0 here.

DNS health is `included / expected` stake, where the denominator is **all active stake** at the
epoch anchor and the numerator is the stake that actually attested. Below 6000 bps the DNS health
leaves Active, the beacon stops advancing its seed, the grace window (1 epoch on this preset) is
exceeded, and **the PALW lane closes for the whole network**. Bonding without running an
attestation sidecar is what causes this — a bond that never attests still counts in the
denominator.

If you bond, run `kaspa-pq-validator run` alongside the node. Beacon commit/reveal is the node's
own `--enable-beacon`; attestation is the sidecar's. They are not the same thing.
