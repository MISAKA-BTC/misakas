# MISAKA VLT — activation guard (`AwaitingEligibleSnapshot`) design v0.1

Status: **proposed**. Implementation is step 5 of the recovery sequence; steps 1–4 (dependency
horizon, accumulator reindex, quota slots, recovery check) are prerequisites and this document
assumes them landed.

## 1. The failure this prevents

Today the weight fence is a **forced switch**: at `DAA >= vlt_activation_daa_score` the quorum
denominator becomes VLT weight, whatever that weight is. On the devnet of 2026-08-09 it was zero,
and the result was a closed loop:

```
DAA >= weight fence  →  W(E) = 0  →  no epoch reaches Q(E)  →  no anchor is DNS-confirmed
        ↑                                                              ↓
        └────────────── credit needs a canonical anchor ───────────────┘
```

The base ledger kept advancing on PoW — the overlay is liveness-first, so nothing halted — but the
overlay could not climb out, and the only symptom was `reason=zero_total_weight`. A fence that
opens onto an empty table is a one-way door.

The guard makes the fence **the earliest point at which the switch may happen**, not the point at
which it does.

## 2. Eligibility is not "the credit table is non-empty"

`credit_table.is_empty()` is far too weak. One validator holding one job would satisfy it, and
`Q(E) = ⌊2W(E)/3⌋ + 1` is a *fraction* — at W(E) = 50 VLT the quorum is 34 VLT and a single
validator finalizes the chain for everyone. That is the vacuous case `min_network_compute` already
exists to exclude, arriving through a different door.

A snapshot is eligible when **all** of:

| condition | why |
|---|---|
| `resolution_complete` | no certificate in the window was skipped for an `Incomplete` reason (step 1). A snapshot that is missing dependencies it could not load is not a smaller answer, it is an unknown one |
| `total_effective_weight >= min_network_compute` | §4: the set transition is deferred below `W_min`, not taken with whatever is there |
| `validators_with_credit >= min_active_validators` | weight concentrated in one validator is not a network's compute, whatever its magnitude |
| `source_anchor` is DNS-confirmed | the snapshot must be a function of the shared prefix, or two branches disagree about the denominator — see §5 |
| every credit passed its challenge window | already enforced by `aggregate_compute_credits`; restated here because eligibility must not be a second, looser path to the same number |

`total_effective_weight` is **after** the `λ·B_i` bond cap, i.e. `Σ_i min{C_i(E), λ·B_i(E)}` — the
weight that will actually be voted with, not the credit that was minted.

`min_active_validators` is a new `VltParams` field. Proposed default: `min_verifier_confirmations
+ 1` (= 4), the same "one executor plus a confirming committee" shape `min_network_compute` is
derived from, so the two thresholds cannot drift apart in meaning.

## 3. State machine

`VltActivationState` today is **derived per recompute** and not persisted. The guard adds a state
that must survive restarts and be identical across nodes, so the derived enum grows two variants and
one new **persisted** record.

```
PreShadow ──> Shadow ──> AwaitingEligibleSnapshot ──> ActivationScheduled ──> Active
                                  ↑                                             │
                                  └────────── never ────────────────────────────┘
                                                                                │
                                                                          Recovery
```

- **`AwaitingEligibleSnapshot { weight_fence_daa, total_weight, min_network_compute, blocker }`** —
  replaces today's `FenceReachedNoSnapshot`. Bootstrap (bonded-stake) weight continues. `blocker`
  names which eligibility condition failed, so the log says *why* it is still waiting rather than
  restating that it is.
- **`ActivationScheduled { activation_epoch, source_anchor, snapshot_epoch, snapshot_root,
  validator_set_root, total_weight, quorum_weight }`** — an eligible snapshot has been found and
  committed; the switch happens at the next epoch boundary. Bootstrap weight is still in force.
- **`Active { activation_epoch, snapshot_root, … }`** — as today.
- **`Recovery`** — as today, reachable only *from* `Active`.

### The one-way property

`Active → AwaitingEligibleSnapshot` must not exist. Falling back to bonded stake because weight
dipped would put two different authorities on the same chain, and a fork that could choose which
one it liked. Below `W_min` after activation the network holds the last finalized anchor
(`Recovery`) — the paper's recovery rule — and does not silently re-denominate.

## 4. Activation happens at an epoch boundary, never mid-epoch

The validator set and its weights are fixed within an epoch, and compute finalized in epoch `E` is
usable from `E+1` at the earliest. So the sequence is:

```
epoch E    eligible snapshot S_E observed under bootstrap finality
           → commit ActivationScheduled(activation_epoch = E+1, S_E)
epoch E+1  boundary → every node switches to S_E
```

The activation record is committed to the DNS consensus state (`DnsState`, whose `health` field
already establishes the append-last pattern for layout changes), keyed by anchor like the rest of
it. Deciding this from a local DB instead would give "I saw the credit" / "I did not" between nodes
at the same DAA, which is the worst of both a ledger and a committee.

## 5. The hard part: the decision must be a function of the shared prefix

This is the part worth arguing about before writing code.

`vlt_epoch_snapshot` already pins at a block and evaluates every DAA-stamped test at or below
`pin_daa_score`, so two branches derive a byte-identical table *for the same pin*. The activation
decision must inherit exactly that discipline:

- the eligibility test runs against the snapshot pinned at the **DNS-confirmed anchor**, not at the
  sink — the sink differs between branches by construction;
- `ActivationScheduled` is written only when that anchor is itself confirmed, so the record is a
  fact about the shared prefix rather than about whichever tip a node happened to have;
- a node that restarts mid-schedule re-derives the same record from the same anchor, and a node in
  IBD arrives at it when it reaches that anchor.

If those hold, activation is deterministic. If any of them is fudged, the guard turns a liveness
bug into a consistency bug, which is a strictly worse trade.

## 6. Logging

The fence and the activation stop being the same event, so they stop being the same line:

```
[vlt-weight-fence-reached]   daa=… fence=…
[vlt-activation-delayed]     reason=no_eligible_snapshot blocker=below_min_network_compute
                             total_weight=… minimum_weight=… weight_source=bootstrap
[vlt-activation-scheduled]   activation_epoch=… source_anchor=… snapshot_root=…
                             total_weight=… quorum_weight=…
[vlt-weight-snapshot-activated] epoch=… snapshot_root=…
```

`[vlt-activation-delayed]` is the operator's line: it fires once per epoch while waiting and names
the single condition that is not yet met.

## 7. Test matrix

| test | asserts |
|---|---|
| `activation_waits_below_min_network_compute` | `W_min − 1` schedules nothing; `W_min` schedules |
| `activation_waits_below_min_active_validators` | one validator with all the weight is not eligible |
| `activation_waits_on_incomplete_resolution` | a snapshot with an `Incomplete` skip never activates |
| `activation_takes_effect_at_the_next_epoch_boundary` | scheduled in `E`, live in `E+1`, not before |
| `activation_record_survives_restart` | same record re-derived, no second schedule |
| `activation_is_identical_across_branches` | two branches sharing the anchor schedule identically |
| `active_never_falls_back_to_bootstrap` | post-activation weight loss reaches `Recovery`, not `Awaiting` |
| `ibd_node_reaches_the_same_activation_epoch` | a node syncing later agrees |

## 8. Open questions

1. **`min_active_validators` default.** `min_verifier_confirmations + 1` = 4 is proposed above. On a
   five-node devnet that is 80% of the set, which is strict; it is the right shape for mainnet but
   may need a devnet-preset override to be testable at all.
2. **Does `ActivationScheduled` need to be cancellable?** If the snapshot stops being eligible
   between `E` and `E+1` (a challenge succeeds and zeroes a credit), the schedule points at a
   snapshot that no longer qualifies. Proposal: re-evaluate at the boundary and fall back to
   `AwaitingEligibleSnapshot` **if activation has not yet happened** — cancelling a schedule is
   safe, un-activating is not.
3. **Existing networks.** Every shipped preset has the fence at `u64::MAX`, so no live network
   changes behaviour. The devnet presets do change, which is the point.
