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
| the floor alone | 184,456 | 18 |
| the whole step space (`PALW_STEP_MAX_LEAVES`) | 4,194,304 | **22** |

**Four extra rounds buys every class that could ever be adjudicable** — every one, because nothing
deeper than the cap is admissible at all (`worst_case_step_leaf_count_v1` refuses it), so this
ladder cannot fail to reach a class that exists. `window_court` is `rounds × turn_deadline_daa`, so
the cost is four turn deadlines of worst-case prosecution, paid only when a court actually runs to
its worst case.

> *Corrected 2026-08-21, by the test that pins it.* An earlier draft of this table read 47,020 / 16
> rounds / six extra, taken from the floor's **declared** 64/64 job. The ladder is checked against
> the longest job a class ADMITS — its whole context as prefill — which for the floor is 184,456.
> Using the declared job understates a class's own ladder, which is the same mistake as admitting a
> class an attacker picks the job length for.

> **Decision needed at genesis:** set the RC's `court.max_step_leaf_count` to `PALW_STEP_MAX_LEAVES`
> rather than to the floor's own worst case. This is the only part of the plan that expires. Pinned
> in code as `palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT`.

## 5. The second class exists now — and its declared context is not adjudicable

`palw_qwen25_profile` landed on the integration branch the same day, with the geometries measured
from Hugging Face's `config.json` and the real `safetensors` header: `QWEN25_1_5B` (28 layers,
1536, 8960, 12 heads / 2 kv) and `QWEN25_3B` (36, 2048, 11008, 16 / 2). It is a second class over
the same closed catalog, with GQA carried, the RMSNorm gain folded into the following linears, the
QKV biases riding the requantize zero point (an ADR-0040 amendment), and coverage at **100 %**.

Priced against the ladder — `misaka-palw-base0/src/bin/base0-class-sizing.rs` — one number does not
work:

| class | reachable kernels | coverage | longest job (whole context as prefill) |
|---|---|---|---|
| the floor | 9 | PASS | 184,456 — **admissible** (18 rounds) |
| Qwen2.5-1.5B as shipped | 10 | PASS | 132,354,910 — **inadmissible** (cap 4,194,304) |
| Qwen2.5-3B as shipped | 10 | PASS | 219,703,654 — **inadmissible** |

**Coverage cannot see this.** A4 asks whether every kernel the graph reaches is adjudicable, and
the answer is yes for both. What refuses is the leaf count: `worst_case_step_leaf_count_v1` counts
the whole context as prefill, because the ladder must reach the longest job a class ADMITS rather
than the one it typically runs.

`tile_len` is the only knob that moves it, and it buys context in exchange for court granularity
(a dispute localises to a tile, so a tile is how much arithmetic one terminal adjudication redoes):

| `tile_len` | floor | Qwen2.5-1.5B | Qwen2.5-3B |
|---|---|---|---|
| 128 (shipped for Qwen) | 9,229 | 244 | 155 |
| 2048 | 31,919 | 2,179 | 1,574 |
| 16,384 | 51,347 | **4,838** | 3,745 |
| 65,536 (`PALW_STEP_MAX_TILE_LEN`) | 57,456 | 5,532 | **4,289** |

At their declared `n_ctx` of 4096: **1.5B needs `tile_len` 16,384** and **3B needs 65,536**, which
is the maximum the type allows — the 3B class at 4096 sits on the ceiling with no headroom. Either
the tile grows or the context shrinks; `the_shipped_qwen_tile_len_does_not_admit_its_own_declared_context`
is the tripwire, and it fails the moment either constant moves.

Court cost stays independent of model size regardless of which is chosen: `PalwProvenOperandsV1`
verifies the refuter's weight rows against `artifact_root`, so an adjudicating node holds the root
and never the weights. A larger tile costs one terminal adjudication more arithmetic, not every
node more storage.

## 6. Remaining work, in order

1. **Choose the genesis ladder** (§4). Expires at genesis.
2. **PTQ pipeline** for whichever geometry the second class takes —
   `docs/palw-base0-ptq-pipeline-scope.md`, whose §5 engine gaps (norm gain via `MulElem`, GQA,
   per-channel requant) are what decide how faithful a dense-Qwen port can be.
3. **A carrier for `ClassRegistered`**, calling §3's gate before the transition. Belongs with the
   ADR-0046 carriage. Until it exists the pipeline **refuses** the object rather than passing it:
   `palw_v2_validate_objects` had a catch-all that let an unlisted object through unchecked, which
   would have installed a class with no coverage, ladder or pwu check — the A4 hole in the one place
   A4 is decided. The refusal names what a carrier must add.
4. The floor's `residual_requant`, settled with its weights and before its class id (§1).
