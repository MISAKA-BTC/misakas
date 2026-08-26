# The second class's weight — measured, and what it decides

**Gate 3 of road-to-mainnet is "a second class holds weight."** This is the measurement that was
missing, taken on 2026-08-26 against the running testnet-11 and the hardware its producers run on.

Everything below is a number this repository or that chain produced today. The tool that prints the
structural half is `misaka-palw-base0/examples/class-weight-report.rs`; run it where the producers
run. Nothing here is an estimate unless it says so.

**The short answer: no — and the four reasons are facts, not caution.** The class that exists cannot
produce a block and cannot be repaired; it could not carry meaningful weight even if it could; the
unit weight is denominated in does not measure work; and a running chain hands a second class the
minimum grantable share and has no arithmetic that could ever raise it. **Gate 3 is a mint decision,
and on testnet-11 it is already spent.**

> **Read this first, if you are reading it after 2026-08-26.**
>
> "The second class" in this document is **Family M / CAT-M-0001** (`682756bc…`), the pinned-GGUF
> class that testnet-11 registered on 2026-08-24. Later the same day, `palw-base0-runtime-hardening`
> withdrew that family entirely (ADR-0053, `a2dd4459` + `243c77ed`): one execution family, the
> deterministic-integer one, and `verify_class_admission_v2` with no arm around it. So on any branch
> that carries ADR-0053, "the second class" means ADR-0052's `PALW-QWEN36` — **same family as the
> floor, adjudicable, not a black box**. Do not carry §2's conclusions across to it: §2 is about a
> class with no court, and Qwen3.6 has one.
>
> **What does not change with the family:** §3 (`pwu_per_inference` is court granularity, not work
> — 31.6× across tiles for identical compute), §4 (testnet-11's frozen ceilings admit a Qwen class
> only at tile 64 / n_ctx 90), §5 (the fast integer kernels are aarch64-only), and §6 (a running
> chain hands an entrant the minimum grantable share and holds no arithmetic that could raise it).
> Those are statements about the integer family and the ruleset, and ADR-0053 does not touch them.
> §6's dead `FamilyShareCap` is cited by ADR-0053 as the first of its three unimplemented mechanisms.
>
> **And this document still describes a chain that is running.** Withdrawing a family from the code
> does not retire a live registration: as of today's dump, class `682756bc…` is still `Active` on
> testnet-11 holding `share=1, budget=1`, and it is still unable to produce for the reason §1 gives.
> It stops being true of that network when testnet-11 is re-minted, not when the branch lands.

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

Same model, same canonical job, same arithmetic, one profile — only the tile changes
(Qwen2.5-1.5B **A16**, measured on `palw-mainnet-rc-integration`):

| tile_len | pwu/inference (8+4) | worst-case leaves at n_ctx 2048 |
| ---: | ---: | ---: |
| 64 | 385,024 | ≥102,730,988 (over) |
| 128 | 192,764 | ≥51,408,502 (over) |
| 256 | 96,804 | ≥25,747,260 (over) |
| 512 | 50,250 | ≥13,174,686 (over) |
| 1024 | 28,376 | ≥7,148,498 (over) |
| 2048 | **16,038** | 3,875,306 |
| 4096 | 12,194 | 2,670,840 |

Exactly halving per doubling, and independent of `n_ctx` — tile 64 claims 385,024 at n_ctx 90 and at
n_ctx 2048 alike, because the canonical job is 8+4 either way.

**A 31.6× spread in claimed weight for identical compute.** `pwu_per_inference` is the step-leaf
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

Decode throughput at the real Qwen2.5-1.5B shape (`base0-throughput`, 28 layers, 12 tokens), on the
same two machines every other timing here was taken on:

| tier | M4 Pro | idle EPYC fleet host | ratio |
| --- | ---: | ---: | ---: |
| W8A8, scalar | 473.7 ms/token | 1409.2 ms/token | 3.0× |
| W8A16 | **26.3 ms/token** (37.98 tok/s, 58.62 GMAC/s) | **967.7 ms/token** (1.03 tok/s, 1.60 GMAC/s) | **36.8×** |

The 18× the Mac gains from W8A16 is the *kernels*, not the tier: `misaka-palw-base0/src/kernels.rs`
gates every fast path on `#[cfg(target_arch = "aarch64")]` with `dotprod` and `i8mm`, and x86 falls
through to the scalar reference — which is why W8A16 buys x86 only 1.5×.

**This is not a liveness problem; it is a fairness one.** A canonical 8+4 inference is 12 tokens, so
at the rates above it costs an x86 producer ≈11.6 s and an Apple-Silicon producer ≈0.32 s — computed
from the per-token measurement, not timed as one run, and excluding the leaf commitment, which at
tile 2048 is 16,038 leaves and small beside either. A producer gets one inference per template and
4,000,000 nonces against it. Both fit a 120 s cadence with room to spare — but within
one class the ticket lottery is a race, and the class retarget only equalises *between* classes.
**An M-series host would take roughly 36× the tickets per second of an EPYC host at the same
class.** The fleet is EPYC; the machine this tier was developed on is not.

Family M pays a second cost the integer family does not: `MetalBackend::verify_material` answers by
**re-running the job** (`self.execute(...)`), because the family has no cheaper check. Every seat
pays the full ≈5.6 s on every claim. `Base0Backend::verify_material` decodes and compares roots.

---

## 6. A running chain cannot raise a class's share

This is the part that decides where the answer can even be given.

`granted_share_table_v2` is, by its own doc, *"the ONLY arithmetic that ever moves a permille"*, and
`write_share` has exactly two callers: the `ClassRegistered` transition and the activation edge that
pays out the share that registration recorded. There is no third.

And the acceptance gate pins what a post-genesis registration may ask for
(`virtual_processor/processor.rs`, the `ClassRegistered` arm):

```rust
let floor = state_params.min_grantable_share_permille();
if *share_permille != floor { return Err(… "a post-genesis entrant joins at the
    minimum grantable share ({floor}‰) — ADR-0049 Decision H" …); }
```

Not *at most* the floor — **exactly** it. Which is why the live table reads `share=1`: 1‰ is
testnet-11's `min_grantable_share_permille`, and it is the only number a second class on this chain
was ever able to hold.

So: **"does the second class get weight" is a mint question, not an operations question.** A share
worth having has to be written into a genesis card. On testnet-11 that decision is already spent.

One latent gap belongs here, because it becomes live the moment a mint hands a second class real
share: `PalwClassAdmissionError::FamilyShareCap` — *"ADR-0051 Decision 1. Family M's classes are
capped at half the share table, so the half that can convict a liar stays in charge of the tie"* —
is **declared and never constructed**, on the deployed build and on `origin/main` alike. Today the
forced-minimum share makes it unreachable, so nothing is wrong on this chain. A genesis card that
grants a non-adjudicable family 600‰ would be accepted.

---

## 7. The decision

1. **Family M gets no weight, and should not keep the producer pointed at it.** It is unbacked (no
   court), mispriced by three orders of magnitude (§2), and permanently unproducible (§1). Every
   attempt its seat makes is a worker spawn that cannot succeed — about 2.3 per second, on the host
   that is already the fleet's OOM risk. Re-mint it as a *new class id* only behind `28f1f623`.
2. **The second weight-bearing class is Qwen2.5-1.5B A16 at tile 2048 / n_ctx 2048** — the only
   candidate that is court-adjudicable, fidelity-green (44/57 top-1, ρ 0.863), and inside the `2^22`
   ladder (3,875,306 leaves, 7.6% headroom).
3. **It cannot be registered onto testnet-11, and could not be given a share if it were.** It needs
   `max_opening_bytes ≥ 37,748,736` (36× what t11 declares) and `max_terminal_macs ≥ 37,748,736`
   (2.2×), both frozen inside `palw_ruleset_id_v2` — and §6 says a running chain hands an entrant
   the minimum share and nothing else. **Gate 3 closes on the next mint.** Those two ceilings are
   the fourth pre-mint cost-ceiling number, now measured rather than guessed.
4. **Before that mint, close the unit.** `pwu_per_inference` varies 30× with tile for identical
   compute and is denominated differently per family (§3). A share granted against it is arbitrary
   in exactly the direction a registrant would choose. Raise `FamilyShareCap` from a message to a
   check in the same pass (§6).
5. **And land an x86 integer kernel before the share is worth racing for.** Not because the fleet
   cannot serve the class — 11.6 s an inference fits a 120 s cadence — but because an M-series host
   would out-ticket it 36 to 1 inside the class (§5).

## 8. What would change this answer

* an x86 integer kernel, which turns (5) from a fairness hole into a number;
* a commensurable work unit — or an explicit rule that `safe_weight` only sums adjudicable families,
  which would make Family M's 4 harmless instead of merely tiny;
* `28f1f623` merged, which is what lets a Family-M model be re-registered when its runtime moves
  rather than dying with the binary it was minted against.

---

## Reproducing this

```bash
cargo run --release -p misaka-palw-base0 --example class-weight-report -- --reps 3
```

Timings are host-dependent by design — run it where the producers run. `--skip-timing` prints the
structural half, which is not. The class ids it derives are a check on itself: at `3aa28476` they
are the two the running chain prints, and if they are not, the binary is pricing a different
network than the one being asked about.

Family M's own numbers come from the deployed worker
(`palw-worker --mode v2-manifest`, `--mode self-job --prompt-stdin --n-predict 12`) and the A16
throughput from `base0-throughput` on `palw-base0-runtime-hardening`, which is where the A16 tier
lives; neither is on `origin/main`.
