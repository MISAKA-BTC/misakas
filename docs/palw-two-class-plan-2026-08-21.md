# The two-class plan: ship the floor, add Qwen scale later

**Status:** decision recorded, mechanism landed, one genesis parameter still to be chosen.
Deliberately not numbered as an ADR — the 0043/0045 numbers have collided twice with a parallel
lane; promote this when a number can be taken safely.

**The plan.** Ship `PALW_RC_BASE0_GEOMETRY` — 4 layers, `d_model` 256 — as the RC's liveness floor,
unchanged. It is ADR-0039 §2a's slow floor and it is not a performance claim. Add a Qwen-scale
BASE-0 class afterwards, once the depth question and the PTQ pipeline are settled.

Checking whether the tree supports the second half found that it did not, and the three reasons are
worth separating because only one of them is a real obstacle.

---

## 1. The floor ships unchanged, and it is comfortably adjudicable

A class is admissible only if its **longest** job — the whole context as prefill plus one decode —
fits `PALW_STEP_MAX_LEAVES` (2²²). Checking the typical job instead would admit a class an attacker
picks the job length for (`worst_case_step_leaf_count_v1`'s own note).

At the floor's geometry and `tile_len` 64 the class is adjudicable up to **n_ctx 5977**, against a
declared 512. Its canonical job (8 prefill / 4 decode) is 3,924 leaves; its declared worst case
(64/64) is 47,020. Nothing here is close to a limit.

**Nothing to change.** One open item, from the depth measurement rather than from this: at 4 layers
the residual stream is 8 adds deep against a 7-add memory, so layer 0 reaches the output as ±1. The
`residual_requant` value is an artifact field, so it is settled when the weights are produced — but
it is inside `execution_class_id`, so it is settled *before* the class is frozen, not after.

## 2. "Register it weightless, activate it later" is not available — by design

`granted_share_table_v2` refuses a share below `min_grantable_share_permille()`, which is
`⌈10⁶ / (tolerance · epoch)⌉` floored at 1. The reason is stated where the floor is defined: a zero
share is a zero epoch budget, and a class that can never mine is *"a dead class registered as if it
worked"*.

So the honest version of the plan is **register at the minimum grantable share** — one permille,
funded from the incumbents by largest-remainder donation. That is the smallest weight the ruleset
admits, not none. It is arguably better than zero: a class with 1‰ produces, and a class that
produces can be watched before it is grown.

## 3. The genesis catalog cannot hold a class that does not exist yet

`class_catalog_root` is a `PalwConsensusParamsV2` field and the bundle is what `palw_ruleset_id_v2`
hashes. `verify_palw_genesis_v2` requires each registration's `artifact_root` to equal its catalog
entry's, and `artifact_root` is the weights. A class with no weights therefore cannot be in the
genesis catalog, and under the genesis gate alone **a second class is a flag day**.

And there was no other gate: no `ClassRegistered` producer exists outside the genesis object list
and the test modules. Post-genesis class registration had no path at all.

**Landed:** `consensus/core/src/palw_class_admission_v2.rs` —
`verify_class_admission_v2(bundle, profile, canonical, registration)`. It restates the genesis
loader's checks against one registration and the shape profile it carries, deriving rather than
reading: the reachable kernel set from the profile's own nodes, both leaf counts from `palw_step`,
and the `PalwClassCatalogEntryV2` it returns is the same object the genesis path produces so the two
lanes cannot drift. A registrant supplies only what no function can invent — `artifact_root`, the
economics, and the canonical job — and `pwu_per_inference` is **checked against the count**, because
pwu multiplies fork-choice weight directly.

Consensus-inert: nothing calls it, because `ClassRegistered` still has no carrier. Wiring it is the
carriage lane's work.

## 4. The one real obstacle — and the genesis parameter it forces

`court.max_step_leaf_count` is a bundle field, so **a class deeper than the ladder cannot join a
running chain**; it needs a new ruleset. Unlike the two above, this one cannot be repaired later.

But the ladder is `ceil(log2(leaves)) + terminal` **rounds**:

| ladder provisioned for | max_step_leaf_count | rounds |
|---|---|---|
| the floor alone | 47,020 | 16 |
| the whole step space (`PALW_STEP_MAX_LEAVES`) | 4,194,304 | **22** |

**Six extra rounds buys every class that could ever be adjudicable.** `window_court` is
`rounds × turn_deadline_daa`, so the cost is six turn deadlines of worst-case prosecution — paid
only when a court actually runs to its worst case.

> **Decision needed at genesis:** set the RC's `court.max_step_leaf_count` to `PALW_STEP_MAX_LEAVES`
> rather than to the floor's own worst case. This is the only part of the plan that expires.

## 5. What a Qwen-scale class will actually look like

Measured with `misaka-palw-base0/src/bin/base0-class-sizing.rs`. Three departures from the pinned
Qwen are forced rather than chosen: no GQA (`wk`/`wv` are square), no GatedDeltaNet (BASE-0 is a
plain decoder-only transformer, so the hybrid Qwen3.5 is out and a dense model is in), and a
vocabulary under `MAX_DOT_LEN` — 128,256 fits, Qwen's 151,936 does not.

At 28 layers / `d_model` 1536 / `d_ff` 8960 / vocab 128,256: **1.81 GB** of int8 weights, and the
graph reaches **9 kernels, all catalogued — coverage PASSES.**

The binding parameter is `tile_len`, which buys context and pays in court granularity:

| `tile_len` | max adjudicable n_ctx — floor | — Qwen scale |
|---|---|---|
| 64 | 5977 | **175** |
| 512 | 19,170 | 1,339 |
| 2048 | 31,919 | **4,261** |

So a Qwen-scale class wants `tile_len ≈ 2048` for a 4096 context. One terminal adjudication then
redoes 2048 output elements instead of 64 — at `d_ff` fan-in that is ~18 M int8 MACs, milliseconds
on a CPU, which is the budget the court was always sized for. **Court cost stays independent of
model size** because evidence is proof-carrying: `PalwProvenOperandsV1` verifies the refuter's
weight rows against `artifact_root`, so an adjudicating node holds the root and never the 1.81 GB.

## 6. Remaining work, in order

1. **Choose the genesis ladder** (§4). Expires at genesis.
2. **PTQ pipeline** for whichever geometry the second class takes —
   `docs/palw-base0-ptq-pipeline-scope.md`, whose §5 engine gaps (norm gain via `MulElem`, GQA,
   per-channel requant) are what decide how faithful a dense-Qwen port can be.
3. **A carrier for `ClassRegistered`**, calling §3's gate before the transition. Belongs with the
   ADR-0046 carriage.
4. The floor's `residual_requant`, settled with its weights and before its class id (§1).
