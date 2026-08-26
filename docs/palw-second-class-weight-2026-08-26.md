# The second class's weight — measured, and what it decides

**Gate 3 of road-to-mainnet is "a second class holds weight."** This is the measurement that was
missing, taken on 2026-08-26 against the running testnet-11 and the hardware its producers run on.

Everything below is a number this repository or that chain produced today. The tool that prints the
structural half is `misaka-palw-base0/examples/class-weight-report.rs`; run it where the producers
run. Nothing here is an estimate unless it says so.

**The short answer: no — and the three reasons are facts, not caution.** The class that exists
cannot produce a block and cannot be repaired; it could not carry meaningful weight even if it
could; and the unit weight is denominated in does not measure work. Gate 3 does not close on
testnet-11.

---

## 0. What the chain holds right now

`kaspad --palw-dump-classes` on the testnet-11 producer, at daa 3306:

```
[palw-dump] 2 class(es) at daa 3306
[palw-dump]   class=682756bc…cffd4 base=false status=Active share=1    budget=1
[palw-dump]   class=c185df95…c654a base=true  status=Active share=999  budget=1000
```

and from the floor producer's own log, one line later:

```
[palw-producer] palw weight=18075200 live_total=18950520 final_claims=1144 unresolved=554 courts=0
```

`18,075,200 ÷ 1,144 = 15,800.0` pwu per `Final` claim, and `15,800 = 2 × 7,900`. The floor's
`pwu_per_inference` is 7,900, so **`expected_attempts` is exactly 2** — the class target is still
the `u128::MAX / 2` it booted at, never retargeted, because a 999‰ class that produces ~every block
has nothing to retarget against. The 120 s cadence is paced by Layer-0, not by the class ticket.

The second class has produced **zero blocks** since it was registered on 2026-08-24, and its seat is
spending about 2.3 attempts per second failing.

**The pricing below is the running chain's, not a nearby build's.** The deployed tree is
`/root/misakas-deploy` at `a760c9a1` = `3aa28476` (an ancestor of `origin/main`) plus one kaspad
commit. Built at `3aa28476`, the report derives base class `c185df95…` and second class
`682756bc…` — the two ids the chain just printed. Current `origin/main` derives `cb067cda…`
instead: the checkpoint-state-chunk-map commits moved the floor's class id, so **`origin/main` can
no longer produce for testnet-11's floor.** That is a separate re-mint decision and is out of scope
here, but it is why every number in this document was taken at `3aa28476`.

---

## 1. The second class cannot produce, and cannot be repaired

The node asks its worker for one thing:

```
[palw-producer] the worker refused the job (exit status: 1): [palw-worker] usage: palw-worker
  --mode manifest | --mode self-job|verify --prompt-stdin --n-predict N | --mode v2-job |
  --mode v2-manifest (got mode Some("v2-legs-job"))
```

`misaka-palw-metal` speaks `--mode v2-legs-job`. The deployed worker (`sha256 2bd857f8…`, built
2026-08-14) predates that mode. The repository's worker has it — and that is precisely why it
cannot be deployed:

* `PalwRuntimeManifestV2::manifest_hash()` binds **`worker_binary_sha256`**. The deployed worker
  reports `runtime_manifest_hash_v2 = a324d0bf…`, which is what class `682756bc…` is registered at,
  and `MetalBackend::check_runtime_identity` refuses any worker that reports anything else — *"a
  different runtime is a different class."*
* A rebuilt worker is a different binary, so a different manifest hash, so a different runtime.
* The class id is the **shape profile** id, which does not move when the binary does. So the new
  worker's class id is still `682756bc…`, and `PalwStateV2Error::DuplicateClass` refuses to
  register it again.

**There is no configuration, rebuild, or restart that makes `682756bc…` produce a block.** It is a
permanently dead seat holding 1‰ of the cadence and 1 block of every epoch's budget. The escape is
a *different* class id, which means a different shape profile — which is what the unmerged
`28f1f623` ("a class id must be able to tell two runtimes of one model apart") exists to make
possible.

---

## 2. It could not carry meaningful weight even if it produced

Family M is `PalwExecutionFamilyV1::MetalGguf`, and `is_court_adjudicable()` is `false` for it —
not "unimplemented", but unavailable in principle: the family verifies inside a tolerance, and a
tolerance cannot separate *lied by ε* from *rounded by ε*. A fraudulent Family-M claim can never be
convicted, only fail to gather a quorum. Weight granted on it is unbacked.

And its weight would be numerically meaningless. `family_m_post_genesis_registration_v1` prices an
inference as `pwu_per_inference = pins.exact_decode_tokens` — **4**, the tokens the canonical job
decodes. `safe_weight` adds `claim.pwu` with no family filter, so those 4 land in the same
accumulator as the integer family's step-leaf counts:

| | pwu/inference | weight per block (at the live `expected_attempts = 2`) | s/inference on the fleet host |
| --- | ---: | ---: | ---: |
| floor `c185df95…` | 7,900 | 15,800 | **0.083** |
| Family M `682756bc…` | 4 | **8** | **≈5.6** (2.9 s of it model load) |

Family M does roughly **67× the real work per inference and is paid 1/1975 of the weight.** At its
1‰ share that is 8 pwu per 33-hour epoch against the floor's ~15.8 M — five parts in ten million.

The Family-M timing is from the deployed worker on the fleet host itself
(`palw-worker --mode self-job --prompt-stdin --n-predict 12`, 8-token prompt): 5.61 s / 5.74 s /
5.61 s total, of which `model loaded in 2.85 s` — the worker is spawned per job, so **half the cost
is reloading the model**. A 68-token run took 80.3 s.

---

## 3. `pwu_per_inference` is court granularity, not work

Same model, same canonical job, same arithmetic — only the tile changes (Qwen2.5-1.5B A16,
`palw-mainnet-rc-integration`):

| tile_len | pwu/inference (8+4) | worst-case leaves |
| ---: | ---: | ---: |
| 64 | 366,184 | 2,978,036 |
| 512 | 50,250 | ≥13,174,686 (over) |
| 1024 | 28,376 | ≥7,148,498 (over) |
| 2048 | **16,038** | 3,875,306 |
| 4096 | 12,194 | 2,670,840 |

**A 30× spread in claimed weight for identical compute.** `pwu_per_inference` is the step-leaf
count: how finely the execution is committed, which is a *court* property. The admission gate is
right to recount it rather than let a registrant declare it — but recounting a number that does not
measure work only makes the wrong quantity trustworthy.

This is not academic. It is the reason the two integer Qwen classes look 65× heavier than the floor
in §4 while being the same order of magnitude of arithmetic per token, and it is why **any share
granted against today's `pwu_per_inference` is arbitrary.** A second class carrying real weight
should wait for a unit that is comparable across tiles and across families, or for a rule that keeps
non-adjudicable families out of the sum entirely.

The leaf count is also a producer cost, not only a court one: at tile 64 / n_ctx 90 an inference
commits 512,160 leaves, and that hashing is a large share of the 30.8 s in §4.

---

## 4. testnet-11 cannot host a Qwen class worth weighting

The court testnet-11 shipped, and what it admits:

| ceiling | testnet-11 |
| --- | ---: |
| `max_step_leaf_count` (the ladder) | 4,194,304 |
| `max_opening_bytes` | 1,048,576 |
| `max_terminal_macs` | 16,777,216 |
| `max_operand_count` | 8 |

| tile_len | widest admissible n_ctx | refused by |
| ---: | ---: | --- |
| 64 | **90** | — |
| 128 … 65536 | — | `max_opening_bytes`, every one of them |

So the only Qwen geometry this chain will ever admit is **tile 64 / n_ctx 90**, and at that
geometry the ladder is 99.9% spent (4,190,204 of 4,194,304). These are bundle fields inside
`palw_ruleset_id_v2`: **a running chain cannot raise them.**

The classes, priced at the deployed parameters (timings: M4 Pro / an idle 8-vCPU EPYC fleet host):

| class | family | court? | geometry | pwu/inf | worst-case | coverage | weights | s/inf (M4) | s/inf (x86) |
| --- | --- | :-: | --- | ---: | ---: | :-: | ---: | ---: | ---: |
| PALW-BASE-0/rc `c185df95…` | integer | yes | tile 64 / ctx 512 | 7,900 | 481,424 | PASS | derived | 0.034 | **0.083** |
| Qwen2.5-1.5B `0bbc807c…` | integer | yes | tile 64 / ctx 90 | 512,160 | 4,190,204 | PASS | 1.65 GiB | 12.305 | **30.838** |
| Qwen2.5-3B `12a56e82…` | integer | yes | tile 64 / ctx 56 | 820,968 | 4,179,788 | PASS | 3.16 GiB | 25.242 | **63.008** |
| CAT-M-0001 `682756bc…` | MetalGguf | **no** | ctx 4096 | 4 | n/a | n/a | 1.19 GiB | — | ≈5.6 |

A 90-token context is not a language model anybody would register a class for. And the arithmetic
tier this line ships is W8A8, whose fidelity gate is **red**: ADR-0047's own fake-quantization
ladder scores the static int8 engine at **4/57 top-1** against the exact model, the same score as
6-bit activations, because static scales and multi-stage requantization cost about two bits. W8A16
scores 44/57 top-1, 56/57 top-5, ρ 0.863 (held-out 39/48, ρ 0.849) and is the only tier that passes
the gate — and it is on neither `origin/main` nor the deployed build. A class registered here would
do real, adjudicable work and emit text nobody should read.

### What the geometry worth having would have cost

Qwen2.5-1.5B A16 at tile 2048 / n_ctx 2048 — the pairing whose fidelity gate is green and which
fits the ladder with 7.6% headroom (3,875,306 of 4,194,304):

| | required | testnet-11 declares | over |
| --- | ---: | ---: | ---: |
| `max_opening_bytes` | 37,748,736 | 1,048,576 | **36.0×** |
| `max_terminal_macs` | 37,748,736 | 16,777,216 | **2.2×** |
| `max_operand_count` | 3 | 8 | ok |
| worst-case leaves | 3,875,306 | 4,194,304 | ok |

And the ladder has a second, harder floor under it: `PALW_STEP_MAX_LEAVES` is a **code** constant,
also `2^22`. n_ctx 4096 at tile 2048 needs ≥9,814,842 leaves and is refused by the code, not by the
bundle — so raising a mint's ceilings buys context only up to 2048 without an arithmetic change.

---

## 5. What the fleet can actually run

Decode throughput at the real Qwen2.5-1.5B shape (`base0-throughput`, 28 layers, 12 tokens):

| tier | M4 Pro | GMAC/s |
| --- | ---: | ---: |
| W8A8, scalar | 473.7 ms/token (2.11 tok/s) | 3.26 |
| W8A16 + NEON + rayon | **26.3 ms/token (37.98 tok/s)** | 58.62 |

The 18× is the kernels, not the tier — and `misaka-palw-base0/src/kernels.rs` gates every fast path
on `#[cfg(target_arch = "aarch64")]` with `dotprod` and `i8mm`. **The testnet-11 fleet is x86
EPYC.** An x86 producer gets the scalar path. That is the same shape of fault as §1: a class the
chain admits and the fleet cannot serve.

---

## 6. The decision

1. **Family M gets no weight, and should not keep the seat it has.** It is unbacked (no court),
   mispriced by three orders of magnitude (§2), and permanently unproducible (§1). Leave its share
   at the 1‰ minimum or freeze it, and stop pointing a producer at `682756bc…` — every attempt it
   makes is a spawn that cannot succeed. Re-mint it as a *new class id* only behind `28f1f623`.
2. **The second weight-bearing class is Qwen2.5-1.5B A16 at tile 2048 / n_ctx 2048** — the only
   candidate that is court-adjudicable, fidelity-green, and inside the `2^22` ladder.
3. **It cannot be registered onto testnet-11.** It needs `max_opening_bytes ≥ 37,748,736` and
   `max_terminal_macs ≥ 37,748,736`, both frozen in the ruleset id. **Gate 3 closes on the next
   mint, not on this chain.** Those two numbers are the fourth item of the pre-mint cost-ceiling
   decision, now measured rather than guessed.
4. **Before that mint, close the unit.** `pwu_per_inference` varies 30× with tile for identical
   compute and is denominated differently per family (§3). A share granted against it is arbitrary
   in exactly the direction a registrant would choose.
5. **And before granting it share, land an x86 kernel.** The A16 fast path is aarch64-only (§5);
   the fleet is not.

## 7. What would change this answer

* an x86 integer kernel, which turns (5) from a blocker into a number;
* a commensurable work unit — or an explicit rule that `safe_weight` only sums adjudicable
  families, which would make Family M's 4 harmless instead of merely tiny;
* `28f1f623` merged, which is what lets a Family-M model be re-registered when its runtime moves
  rather than dying with the binary it was minted against.
