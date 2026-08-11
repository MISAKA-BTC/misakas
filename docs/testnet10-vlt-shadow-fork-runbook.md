# testnet-10 VLT SHADOW fork — runbook (ADR-0024 activation step 3)

Status: **prepared 2026-08-11**, fence height `H` deliberately unset — it is chosen at release
cut against the live t10 DAA, not in this document. Steps 1–2 of the ADR-0024 runbook are done
and evidenced (five-validator devnet on the real Qwen3.5-2B profile: 78 committee replays
reproduced `R_j`, weighted finality certified epochs 34+ — commit `5e80356`; canonical
activation record — commit `cd3fb64`).

## What this fork is, and is not

One release moves **both** of these, together, for `TESTNET_DNS_PARAMS`:

| knob | from | to |
|---|---|---|
| `vlt.vlt_shadow_activation_daa_score` | `u64::MAX` | `H` (scheduled) |
| `vlt.model_cost_table` | `EMPTY` | `[palw_qwen36_metal_entry, palw_qwen35_2b_metal_entry]` |

They ship together because separately each is pointless or misleading: a fence with no
registered model runs an overlay in which every job mints zero; a table with no fence changes
nothing (`validator_service` idles the whole compute cycle below `vlt_shadow_active_at` — the
role resolves and logs `enabled:`, then originates, audits and declares nothing).

The weight fence (`vlt_activation_daa_score`) stays `u64::MAX`. **Finality is outside this
fork's blast radius** — that is the entire point of taking shadow first. What becomes real:

* the audit fee moves coinbase value (every counted verdict is paid at its certificate's
  challenge-window crossing), and
* settled compute challenges slash bonds (`ContradictoryVerification` at acceptance; the
  provable-only rule).

So it is a true hard fork — old builds reject the first block whose coinbase pays an audit fee
— with compute-overlay-sized consequences and finality-sized none.

## Preconditions (all currently satisfied except the fleet audit)

1. **IBD equality** — a node that was not present must derive the same overlay. Evidenced by:
   pruning-import node joining live weighted finality with an identical §12 identity tuple
   (`cd3fb64` closed the one field that disagreed), and the from-genesis replay E2E
   (`scripts/misaka-vlt-ibd-e2e.sh`).
2. **`vlt_params_consistent()`** must hold for the edited preset — it asserts shadow ≤ weight,
   the §7 unbonding bound, and the credit-window/soak span. It is checked by `update_dns_state`
   before `Active`, but run the unit suite at release cut rather than discovering it in field.
3. **Fleet audit (open)** — the registered profiles are Apple-Silicon/Metal determinism
   classes. Compute participation needs ≥ 1 executor + 3 same-class verifiers **per class**, or
   every certificate stalls below `min_verifier_confirmations` and mints nothing (safe, but a
   shadow that measures nothing defers step 4 indefinitely). Options, in order of preference:
   a. count ≥ 4 Apple-Silicon validators into the fleet (M-series minis are the cheap path);
   b. add a pinned **Linux/CPU deterministic profile** (fixed threads, `GGML_NATIVE=OFF`,
      same worker contract — a new `ModelCostEntry` + class, its own calibration run);
   c. run shadow with zero compute (legal; proves only the fork mechanics).
   The choice is step 4's hardware-class decision; (a) unblocks measurement soonest.

## Choosing `H`

Same A2 pattern as the 2026-08-10 flag day: `H` ≥ current tip + (fleet update window × safety
2×). Every validator/miner/seeder binary must be **inside the fleet before the flag** — a node
that crosses `H` on the old build forks itself on the first audit-fee coinbase. Use the batch
builds + collector watch exactly as `docs/testnet10-transition.md` records; the collector's
per-host version table is the go/no-go gate.

## The release diff (apply at cut, filling `H`)

```rust
// consensus/core/src/config/params.rs — TESTNET_DNS_PARAMS
vlt: VltParams {
    vlt_shadow_activation_daa_score: H,          // scheduled flag height
    vlt_activation_daa_score: u64::MAX,          // the vote does NOT move in this fork
    model_cost_table: ModelCostTable::palw_metal_registered(), // both pinned Metal profiles
    ..VltParams::INERT
},
```

`min_network_compute` stays at the production default through shadow: it gates **weight-fork
eligibility**, not shadow, and its real value is one of the numbers shadow exists to measure
(step 4 freezes it together with `R0`/`H_halving`/`D_settle` for the token program).

## Operator surface during the soak

Watch, per host:

```
[vlt-shadow] sink_daa=… N validator(s) with credit; Q/K epoch(s) would reach quorum; newest epoch …: W(E)=…
[validator-service] compute: committed to job … / confirmed certificate … — our replay reproduced R_j
[vlt-credit] … certificate(s) in the window, none credited: <reasons>
[vlt-identity] epoch=… <7-field tuple>
```

Exit criteria for scheduling step 4 (the weight fork): a **healthy, boring** `[vlt-shadow]`
line — W(E) above the recalibrated floor with a consistent would-be quorum — across the whole
fleet for at least one full credit window, identity tuples equal on every host including at
least one freshly-synced one, and the step-4 constants frozen from the measurements.

## Rollback

Below the weight fence the overlay cannot stall finality, so the worst credible outcome is
compute-overlay misbehaviour (bad fee flow, spurious slashing). The response is a point release
that pushes `vlt_shadow_activation_daa_score` forward (re-fencing), not a chain rollback;
bonds slashed by a bug would need the §10 operator process. Nothing in this fork can strand
the chain itself.
